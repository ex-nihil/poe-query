#[macro_use]
extern crate pest_derive;
extern crate core;

use std::{env, process};
use std::path::{Path, PathBuf};
use std::time::Instant;

use clap::{CommandFactory, Parser};
use clap::error::ErrorKind;
use log::*;
use poe_bundle::BundleReader;
use simplelog::*;

use crate::dat::DatReader;
use crate::query::Term;
use crate::traversal::{StaticContext, QueryProcessor};
use crate::traversal::value::Value;

mod dat;
mod error;
mod introspect;
mod query;
mod serve;
mod translate;
mod traversal;

const EXIT_SETUP: i32 = 1;
const EXIT_PARSE: i32 = 2;
const EXIT_EVAL: i32 = 3;

#[derive(clap::Parser)]
#[command(name = "PoE Query")]
#[command(bin_name = "poe_query")]
#[command(author = "Daniel Dimovski <daniel@timeloop.se>")]
#[command(version = env ! ("CARGO_PKG_VERSION"))]
#[command(about = "Query and transform data from Path of Exile", long_about = None)]
struct Args {
    /// Path to the Path of Exile installation
    #[arg(short, long, value_name = "INSTALL_DIR", global = true)]
    path: Option<PathBuf>,

    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    #[arg(short, long, default_value = "English", global = true)]
    language: String,

    /// Directory with the dat-schema specifications (default: dat-schema next to the binary)
    #[arg(short, long, value_name = "SCHEMA_DIR", global = true)]
    schema: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Run a query against the game data
    Query { query: String },
    /// List every table available in the schema
    Tables,
    /// Show a table's columns with types, references, and enum values
    Describe {
        table: String,
        /// Full structured output instead of the compact column map
        #[arg(long)]
        full: bool,
    },
    /// Translate stat ids with values into in-game text
    Translate {
        /// Stats as id=value or id=min..max, e.g. base_maximum_life=25
        #[arg(required = true)]
        stats: Vec<String>,
        /// Stat description file base name (default: stat_descriptions)
        #[arg(long)]
        file: Option<String>,
    },
    /// Answer NDJSON requests over stdio (one request per line)
    Serve,
}

fn main() {
    let args = parse_args();
    init_logger(args.verbose);
    debug!("Version {:?}", env!("CARGO_PKG_VERSION"));

    let schema_path = find_schema_path(args.schema);
    info!("Schemas: {:?}", schema_path);

    match args.command {
        Command::Query { query } => run_query(&query, args.path, &args.language, &schema_path),
        Command::Tables => run_tables(&schema_path),
        Command::Describe { table, full } => run_describe(&schema_path, &table, full),
        Command::Translate { stats, file } => run_translate(&stats, file.as_deref(), args.path, &args.language, &schema_path),
        Command::Serve => run_serve(args.path, &args.language, &schema_path),
    }
}

fn run_translate(stats: &[String], file: Option<&str>, path_arg: Option<PathBuf>, language: &str, schema_path: &Path) {
    let stats: Vec<translate::StatValue> = stats.iter().map(|arg| {
        parse_stat_arg(arg).unwrap_or_else(|message| {
            error!("{}", message);
            process::exit(EXIT_PARSE);
        })
    }).collect();

    let install_path = find_poe_install(path_arg);
    info!("Using: {:?}", install_path);
    let bundles = BundleReader::from_install(&install_path);
    let container = DatReader::from_install(language, &bundles, schema_path);

    let descriptions = match container.stat_descriptions(file) {
        Ok(descriptions) => descriptions,
        Err(error) => {
            error!("{}", error);
            process::exit(EXIT_EVAL);
        }
    };

    let translation = descriptions.translate(&stats, language);
    if !translation.unmatched.is_empty() {
        warn!("no description found for: {}", translation.unmatched.join(", "));
    }
    print_json(&translation);
}

fn parse_stat_arg(arg: &str) -> Result<translate::StatValue, String> {
    let (id, value) = arg.split_once('=')
        .ok_or_else(|| format!("expected id=value or id=min..max, got '{}'", arg))?;
    let parse = |text: &str| text.parse::<f64>()
        .map_err(|_| format!("'{}' is not a number in '{}'", text, arg));
    let (min, max) = match value.split_once("..") {
        Some((min, max)) => (parse(min)?, parse(max)?),
        None => {
            let value = parse(value)?;
            (value, value)
        }
    };
    Ok(translate::StatValue { id: id.to_string(), min, max })
}

fn run_serve(path_arg: Option<PathBuf>, language: &str, schema_path: &Path) {
    let install_path = find_poe_install(path_arg);
    info!("Using: {:?}", install_path);

    let now = Instant::now();
    let bundles = BundleReader::from_install(&install_path);
    let container = DatReader::from_install(language, &bundles, schema_path);
    info!("startup: {}ms", now.elapsed().as_millis());

    serve::serve(&container, &install_path);
}

/// A missing or unrecognized subcommand shows the full help (with the
/// subcommand list) instead of just the usage line.
fn parse_args() -> Args {
    match Args::try_parse() {
        Ok(args) => args,
        Err(error) => {
            if matches!(error.kind(), ErrorKind::MissingSubcommand | ErrorKind::InvalidSubcommand) {
                let _ = error.print();
                eprintln!();
                eprintln!("{}", Args::command().render_help());
                process::exit(2);
            }
            error.exit();
        }
    }
}

fn run_query(query: &str, path_arg: Option<PathBuf>, language: &str, schema_path: &Path) {
    let install_path = find_poe_install(path_arg);
    info!("Using: {:?}", install_path);

    // Parse
    let now = Instant::now();
    let terms = match query::parse_query(query) {
        Ok(t) => t,
        Err(error) => {
            error!("{}", error);
            process::exit(EXIT_PARSE);
        },
    };
    let (parse_query_ms, now) = (now.elapsed().as_millis(), Instant::now());

    // Index bundles
    let bundles = BundleReader::from_install(&install_path);
    let container = DatReader::from_install(language, &bundles, schema_path);
    let (read_index_ms, now) = (now.elapsed().as_millis(), Instant::now());

    // Transform
    let context = StaticContext::new(&container);
    let result = match StaticContext::process(&context, &terms) {
        Ok(value) => value,
        Err(error) => {
            error!("{}", error);
            process::exit(EXIT_EVAL);
        },
    };
    let (query_ms, now) = (now.elapsed().as_millis(), Instant::now());

    // Output
    match result {
        Value::Iterator(items) => {
            items.iter().for_each(serialize_and_print);
        }
        _ => serialize_and_print(&result)
    };
    let serialize_ts = now.elapsed().as_millis();

    info!("parse query: {}ms", parse_query_ms);
    info!("bundle index: {}ms", read_index_ms);
    info!("transform spent: {}ms", query_ms);
    info!("serialize spent: {}ms", serialize_ts);
}

fn run_tables(schema_path: &Path) {
    let (specs, _) = dat::load_schema(schema_path);
    print_json(&introspect::table_names(&specs));
}

fn run_describe(schema_path: &Path, table: &str, full: bool) {
    let (specs, _) = dat::load_schema(schema_path);
    let result = if full {
        introspect::describe(&specs, table).map(|description| print_json(&description))
    } else {
        introspect::describe_compact(&specs, table).map(|description| print_json(&description))
    };
    if let Err(error) = result {
        error!("{}", error);
        process::exit(EXIT_EVAL);
    }
}

fn print_json<T: serde::Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(serialized) => println!("{}", serialized),
        Err(error) => {
            error!("{}", error);
            process::exit(EXIT_EVAL);
        }
    }
}

fn serialize_and_print(value: &Value) {
    print_json(value)
}

fn init_logger(verbosity: u8) {
    TermLogger::init(
        match verbosity {
            0 => LevelFilter::Warn,
            1 => LevelFilter::Info,
            2 => LevelFilter::Debug,
            _ => LevelFilter::Trace,
        },
        ConfigBuilder::new()
            .set_thread_level(LevelFilter::Off)
            .set_time_level(LevelFilter::Off)
            .set_location_level(LevelFilter::Off)
            .set_target_level(LevelFilter::Off)
            .build(),
        TerminalMode::Stderr,
        ColorChoice::Auto)
        .unwrap_or_default();
}

fn find_poe_install(path_arg: Option<PathBuf>) -> Box<Path> {
    match path_arg {
        Some(path) => {
            let is_file = path.exists() && path.is_file();
            match is_file || contains_ggpk_or_index(&path) {
                true => Some(path),
                false => None
            }
        }
        None => attempt_to_find_installation()
    }.unwrap_or_else(|| {
        error!("Path of Exile not found. Provide a valid path with -p flag.");
        process::exit(EXIT_SETUP);
    }).into_boxed_path()
}

fn attempt_to_find_installation() -> Option<PathBuf> {
    [
        ".",
        "C:/Program Files (x86)/Grinding Gear Games/Path of Exile",
        "C:/Program Files/Steam/steamapps/common/Path of Exile",
        "/home/nihil/Games/path-of-exile/drive_c/Program Files (x86)/Grinding Gear Games/Path of Exile/"
    ].into_iter()
        .find_map(|p| {
            let path = PathBuf::from(p);
            match contains_ggpk_or_index(&path) {
                true => Some(path.canonicalize().unwrap()),
                false => None
            }
        })
}

fn contains_ggpk_or_index(path: &Path) -> bool {
    let has_ggpk = path.join("Content.ggpk").exists();
    let has_index = path.join("Bundles2/_.index.bin").exists();
    has_ggpk || has_index
}

fn find_schema_path(schema_arg: Option<PathBuf>) -> Box<Path> {
    let schema_dir = schema_arg.unwrap_or_else(|| {
        let mut schema_dir = env::current_exe().unwrap();
        schema_dir.pop(); // remove file
        schema_dir.push("dat-schema");
        schema_dir
    });
    if !schema_dir.is_dir() {
        error!("Schema directory not found at {:?}. Provide one with the -s flag.", schema_dir);
        process::exit(EXIT_SETUP);
    }
    schema_dir.into_boxed_path()
}
