#[macro_use]
extern crate pest_derive;

use std::{env, process};
use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;
use log::*;
use simplelog::*;
use poe_bundle::BundleReader;

use crate::dat::DatReader;
use crate::query::Term;

use crate::traversal::traverse::{SharedCache, StaticContext, TermsProcessor, TraversalContext};
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
    let arg = Args::parse();

    CombinedLogger::init(vec![TermLogger::new(
        match arg.verbose {
            0 => LevelFilter::Warn,
            1 => LevelFilter::Info,
            2 => LevelFilter::Debug,
            3 | _ => LevelFilter::Trace,
        },
        Config::default(),
        TerminalMode::Stderr,
        ColorChoice::Never,
    )]).unwrap_or_default();
    debug!("Version {:?}", env!("CARGO_PKG_VERSION"));

    let query = arg.query;

    let path = find_poe_install(arg.path);
    let specs = dat_schema_path();
    info!("Using {:?}", path);
    info!("Specs {:?}", specs);

    let terms = query::parse(&query);

    let mut now = Instant::now();

    let bundles = BundleReader::from_install(path.as_path());

    let container = DatReader::from_install(&arg.language, &bundles, specs.as_path());
    let navigator = StaticContext {
        store: Some(&container),
    };
    let read_index_ms = now.elapsed().as_millis();

    now = Instant::now();
    let value = navigator.process(&mut TraversalContext::default(), &mut SharedCache::default(), &terms);
    let query_ms = now.elapsed().as_millis();

    now = Instant::now();

    match value {
        Value::Iterator(items) => {
            items.iter().for_each(|item| {
                let serialized = serde_json::to_string_pretty(item).expect("seralized");
                println!("{}", serialized);
            });
        }
        _ => {
            let serialized = serde_json::to_string_pretty(&value).expect("serialized2");
            println!("{}", serialized);
        }
    };
    let serialize_ts = now.elapsed().as_millis();

    info!("setup spent: {}ms", read_index_ms);
    info!("query spent: {}ms", query_ms);
    info!("serialize spent: {}ms", serialize_ts);
}

fn find_poe_install(path_arg: Option<PathBuf>) -> PathBuf {
    match path_arg {
        Some(p) => {
            let is_file = p.exists() && p.is_file();
            let found_ggpk = p.join("Content.ggpk").exists();
            let found_index = p.join("Bundles2/_.index.bin").exists();
            match is_file || found_ggpk || found_index {
                true => Some(p),
                false => None
            }
        },
        None =>
            [
                ".",
                "C:/Program Files (x86)/Grinding Gear Games/Path of Exile",
                "C:/Program Files/Steam/steamapps/common/Path of Exile"
        ].into_iter()
                .find_map(|p| {
                    let path = PathBuf::from(p);
                    let has_ggpk = path.join("Content.ggpk").exists();
                    let has_index = path.join("Bundles2/_.index.bin").exists();
                    match has_ggpk || has_index {
                        true => Some(path.canonicalize().unwrap()),
                        false => None
                    }
                })
    }.unwrap_or_else(|| {
        error!("Path of Exile not found. Provide a valid path with -p flag.");
        process::exit(-1);
    })
}

fn dat_schema_path() -> PathBuf {
    let mut schema_dir = env::current_exe().unwrap();
    schema_dir.pop(); // remove file
    schema_dir.push("dat-schema");
    schema_dir
}