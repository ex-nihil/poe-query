#[macro_use]
extern crate pest_derive;
extern crate core;

use std::{env, process};
use std::path::{Path, PathBuf};
use std::time::Instant;

use clap::Parser;
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
mod traversal;

const EXIT_SETUP: i32 = 1;
const EXIT_PARSE: i32 = 2;
const EXIT_EVAL: i32 = 3;

#[derive(clap::Parser)]
#[command(name = "PoE Query")]
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
    Describe { table: String },
}

fn main() {
    let args = Args::parse();
    init_logger(args.verbose);
    debug!("Version {:?}", env!("CARGO_PKG_VERSION"));

    let schema_path = find_schema_path(args.schema);
    info!("Schemas: {:?}", schema_path);

    match args.command {
        Command::Query { query } => run_query(&query, args.path, &args.language, &schema_path),
        Command::Tables => run_tables(&schema_path),
        Command::Describe { table } => run_describe(&schema_path, &table),
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

fn run_describe(schema_path: &Path, table: &str) {
    let (specs, _) = dat::load_schema(schema_path);
    match introspect::describe(&specs, table) {
        Ok(description) => print_json(&description),
        Err(error) => {
            error!("{}", error);
            process::exit(EXIT_EVAL);
        }
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
        "C:/Program Files/Steam/steamapps/common/Path of Exile"
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
