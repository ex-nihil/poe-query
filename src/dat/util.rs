use std::fs::File;
use std::io::BufReader;
use std::path::Path;

pub fn search_for(data: &[u8], needle: &[u8]) -> usize {
    data.windows(needle.len())
        .position(|window| window == needle)
        .unwrap()
}