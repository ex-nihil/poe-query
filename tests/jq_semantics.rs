mod common;
use common::process;

// Every expectation in this file is the output of the same expression in
// real jq (one entry per output document).

#[test]
fn field_access_distributes_over_a_stream() {
    let result = process(r#"[{"a": 1}, {"a": 2}][] | .a"#);
    assert_eq!(result, vec!["1", "2"]);
}

#[test]
fn iteration_distributes_and_splices() {
    let result = process("[[1,2,3],[4]][] | .[]");
    assert_eq!(result, vec!["1", "2", "3", "4"]);
}

#[test]
fn indexing_distributes_over_a_stream() {
    let result = process("[[1,2],[3,4]][] | .[0]");
    assert_eq!(result, vec!["1", "3"]);
}

#[test]
fn slicing_distributes_over_a_stream() {
    let result = process("[[1,2,3],[4,5]][] | .[0:2]");
    assert_eq!(result, vec!["[1,2]", "[4,5]"]);
}

#[test]
fn length_distributes_over_a_stream() {
    let result = process("[[1,2],[3]][] | length");
    assert_eq!(result, vec!["2", "1"]);
}

#[test]
fn object_construction_builds_one_object_per_element() {
    let result = process(r#"[{"a": 1}, {"a": 2}][] | { b: .a }"#);
    assert_eq!(result, vec![r#"{"b":1}"#, r#"{"b":2}"#]);
}

#[test]
fn array_construction_builds_one_array_per_element() {
    let result = process(r#"[{"a": 1}, {"a": 2}][] | [.a]"#);
    assert_eq!(result, vec!["[1]", "[2]"]);
}

#[test]
fn collecting_a_stream_builds_a_single_array() {
    let result = process(r#"[ [{"a": 1}, {"a": 2}][] | .a ]"#);
    assert_eq!(result, vec!["[1,2]"]);
}

#[test]
fn comma_concatenates_streams() {
    let result = process("[[1,2][], 9]");
    assert_eq!(result, vec!["[1,2,9]"]);
}

#[test]
fn mixed_explicit_and_shorthand_object_keys() {
    let result = process(r#"{"a": 1, "b": 2} | { x: .a, b }"#);
    assert_eq!(result, vec![r#"{"x":1,"b":2}"#]);
}
