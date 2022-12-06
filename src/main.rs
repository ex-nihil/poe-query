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
mod query;
mod traversal;
mod tests;

#[derive(clap::Parser)]
#[command(name = "PoE Query")]
#[command(author = "Daniel Dimovski <daniel@timeloop.se>")]
#[command(version = env ! ("CARGO_PKG_VERSION"))]
#[command(about = "Query and transform data from Path of Exile", long_about = None)]
struct Args {
    #[arg(short, long, value_name = "INSTALL_DIR")]
    path: Option<PathBuf>,

    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[arg(short, long, default_value_t = String::from("English"))]
    language: String,

    query: String,
}

fn main() {
    let args = Args::parse();
    init_logger(args.verbose);
    debug!("Version {:?}", env!("CARGO_PKG_VERSION"));

    let install_path = find_poe_install(args.path);
    let schema_path = find_schema_path();
    info!("Using: {:?}", install_path);
    info!("Schemas: {:?}", schema_path);

    // Parse
    let now = Instant::now();
    let terms = match query::parse_query(&args.query) {
        Ok(t) => t,
        Err(error) => {
            error!("{}", error);
            process::exit(-1);
        },
    };
    let (parse_query_ms, now) = (now.elapsed().as_millis(), Instant::now());

    // Index bundles
    let bundles = BundleReader::from_install(&install_path);
    let container = DatReader::from_install(&args.language, &bundles, &schema_path);
    let (read_index_ms, now) = (now.elapsed().as_millis(), Instant::now());

    // Transform
    let context = StaticContext::new(&container);
    let result = StaticContext::process(&context, &terms);
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

fn serialize_and_print(value: &Value) {
    let serialized = serde_json::to_string_pretty(&value).unwrap();
    println!("{}", serialized);
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
        process::exit(-1);
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

fn find_schema_path() -> Box<Path> {
    let mut schema_dir = env::current_exe().unwrap();
    schema_dir.pop(); // remove file
    schema_dir.push("dat-schema");
    schema_dir.into_boxed_path()
}