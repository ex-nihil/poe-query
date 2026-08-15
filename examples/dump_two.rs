use std::env;

use poe_bundle::{BundleReader, BundleReaderRead};

/// Read two bundle files in sequence to reproduce a heap corruption.
fn main() {
    let args: Vec<String> = env::args().collect();
    let reader = BundleReader::from_install(args[1].as_ref());
    for path in &args[2..] {
        let bytes = reader.bytes(path).unwrap();
        eprintln!("{}: {} bytes", path, bytes.len());
    }
}
