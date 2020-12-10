use clap::{App, Arg};
extern crate pest;
#[macro_use]
extern crate pest_derive;

mod dat;
use dat::traverse::TermsProcessor;
use dat::reader::{DatContainer, DatContainerImpl};
use std::time::Instant;

pub mod lang;
pub use lang::Term;
pub use dat::value::Value;

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
            Arg::with_name("query")
                .value_name("\"QUERY\"")
                .required(true)
                .help("example: .mods.id")
                .takes_value(true),
        )
        .get_matches();

    let query = matches.value_of("query").unwrap();
    let terms = lang::parse(query);
    //println!("TERM: {:?}", terms);

    let mut now = Instant::now();
    let container = DatContainer::from_install("/Users/nihil/code/poe-files", "spec");
    let mut navigator = container.navigate();
    let read_index_ms = now.elapsed().as_millis();

    now = Instant::now();
    let value = navigator.process(&terms);
    let query_ms = now.elapsed().as_millis();

    let serialized = serde_json::to_string_pretty(&value).unwrap();
    println!("{}", serialized);

    println!("setup spent: {}ms", read_index_ms);
    println!("query spent: {}ms", query_ms);
}
