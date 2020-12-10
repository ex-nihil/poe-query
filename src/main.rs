extern crate pest;
#[macro_use]
extern crate pest_derive;
extern crate log;
extern crate simplelog;

use clap::{App, Arg};
use simplelog::*;
use log::*;

mod dat;
use dat::reader::{DatContainer, DatContainerImpl};
use dat::traverse::TermsProcessor;
use std::time::Instant;

pub mod lang;
pub use dat::value::Value;
pub use lang::Term;

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
        TerminalMode::Mixed,
    )])
    .unwrap();

    let query = matches.value_of("query").unwrap();
    let terms = lang::parse(query);
    debug!("terms: {:?}", terms);

    let mut now = Instant::now();
    let container = DatContainer::from_install("/Users/nihil/code/poe-files", "spec");
    let mut navigator = container.navigate();
    let read_index_ms = now.elapsed().as_millis();

    now = Instant::now();
    let value = navigator.process(&terms);
    let query_ms = now.elapsed().as_millis();

    let serialized = serde_json::to_string_pretty(&value).unwrap();
    println!("{}", serialized);

    info!("setup spent: {}ms", read_index_ms);
    info!("query spent: {}ms", query_ms);
}
