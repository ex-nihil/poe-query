use clap::{App, Arg};
extern crate pest;
#[macro_use]
extern crate pest_derive;

mod dat;
use dat::dat_navigate::DatNavigateImpl;
use dat::dat_reader::{DatContainer, DatContainerImpl};

pub mod lang;
pub use dat::dat_file::DatValue;
pub use lang::Term;

fn main() {
    let matches = App::new("PoE DAT transformer")
        .version("1.1.0")
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
    println!("TERMS: {:?}", terms);

    let container = DatContainer::from_install("/Users/nihil/code/poe-files", "spec");

    let mut navigator = container.navigate();
    let value = navigator.traverse_terms(&terms);
    match value {
        DatValue::List(items) => items.iter().for_each(|item| println!("{:?}", item)),
        _ => println!("{:?}", value),
    }
}
