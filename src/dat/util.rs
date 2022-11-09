
pub fn search_for(data: &[u8], needle: &[u8]) -> usize {
    data.windows(needle.len())
        .position(|window| window == needle)
        .unwrap()
}