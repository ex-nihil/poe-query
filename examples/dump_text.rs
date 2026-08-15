use std::env;

use poe_bundle::{BundleReader, BundleReaderRead};

/// Dump a UTF-16 text file from the bundles to stdout.
/// Usage: cargo run --example dump_text -- <install_dir> <bundle_path>
///        cargo run --example dump_text -- <install_dir> --list <prefix>
fn main() {
    let args: Vec<String> = env::args().collect();
    let reader = BundleReader::from_install(args[1].as_ref());

    if args[2] == "--list" {
        let prefix = args[3].to_lowercase();
        for path in reader.index.paths() {
            if path.to_lowercase().starts_with(&prefix) {
                println!("{}", path);
            }
        }
        return;
    }

    let path = &args[2];
    let Some(size) = reader.size_of(path) else {
        eprintln!("not in index: {}", path);
        std::process::exit(1);
    };
    eprintln!("{} bytes", size);
    let bytes = reader.bytes(path).unwrap();
    let text = if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        let units: Vec<u16> = bytes[2..].chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(&bytes).to_string()
    };
    println!("{}", text);
}
