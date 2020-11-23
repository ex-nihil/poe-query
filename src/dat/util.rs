
use std::io::BufReader;
use std::fs::File;
use std::path::Path;

pub fn search_for(data: &[u8], needle: &[u8]) -> usize {
    data.windows(needle.len())
        .position(|window| window == needle)
        .unwrap()
}

pub fn read_from_path(path: &Path) -> BufReader<File> {
    match File::open(path) {
        Ok(file) => BufReader::new(file),
        _ => panic!(format!("File not found '{:?}'", path)),
    }
}
