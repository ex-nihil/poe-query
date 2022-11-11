extern crate log;
extern crate pest;
#[macro_use]
extern crate pest_derive;
extern crate simplelog;


use std::time::Instant;

use clap::{App, Arg};
use log::*;
use poe_bundle::BundleReader;
use simplelog::*;

use dat::reader::{DatContainer};
pub use query::Term;
use crate::traversal::traverse::{SharedCache, StaticContext, TermsProcessor, TraversalContext};
use crate::traversal::value::Value;

mod dat;
pub mod query;
pub mod traversal;

fn main() {
    let matches = App::new("PoE DAT transformer")
        .version("1.0.0")
        .author("Daniel D. <daniel.k.dimovski@gmail.com>")
        .about("Query and transform data from Path of Exile")
        .arg(
            Arg::with_name("path")
                .short("p")
                .long("path")
                .value_name("DIRECTORY")
                .help("Specify location of Path of Exile installation.")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("limit")
                .short("l")
                .long("limit")
                .help("Amount of rows to output. Useful when exploring.")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("v")
                .short("v")
                .multiple(true)
                .help("Sets the level of verbosity"),
        )
        .arg(
            Arg::with_name("query")
                .value_name("\"QUERY\"")
                .required(true)
                .help("example: .mods.id")
                .takes_value(true),
        )
        .get_matches();

    let log_level = match matches.occurrences_of("v") {
        0 => LevelFilter::Warn,
        1 => LevelFilter::Info,
        2 => LevelFilter::Debug,
        3 | _ => LevelFilter::Trace,
    };
    CombinedLogger::init(vec![TermLogger::new(
        log_level,
        Config::default(),
        TerminalMode::Stderr
    )])
    .expect("logger");

    let query = matches.value_of("query").expect("query arg");
    let path = matches.value_of("path").expect("path arg");
    let terms = query::parse(query);
    debug!("terms: {:?}", terms);

    let mut now = Instant::now();

    let bundles = BundleReader::from_install(path);
    let container = DatContainer::from_install(&bundles, "./dat-schema");
    let navigator = StaticContext {
        store: &container,
    };
    let read_index_ms = now.elapsed().as_millis();

    now = Instant::now();
    let value = navigator.process(&mut TraversalContext {
        current_field: None,
        current_file: None,
        dat_file: None,
        identity: None
    }, &mut SharedCache { variables: Default::default(), files: Default::default() }, &terms);
    let query_ms = now.elapsed().as_millis();

    now = Instant::now();

    match value {
        Value::Iterator(items) => {
            items.iter().for_each(|item| {
                let serialized = serde_json::to_string_pretty(item).expect("seralized");
                println!("{}", serialized);
            });
        },
        _ => {
            let serialized = serde_json::to_string_pretty(&value).expect("serialized2");
            println!("{}", serialized);
        },
    };
    let serialize_ts = now.elapsed().as_millis();

    info!("setup spent: {}ms", read_index_ms);
    info!("query spent: {}ms", query_ms);
    info!("serialize spent: {}ms", serialize_ts);
}
