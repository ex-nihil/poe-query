#![allow(dead_code)]
mod common;
use common::process;

// The following tests are all failing as they're features I would like to implement

//#[test]
fn string_interpolation() {
    let result = process(r#"42 | "The input was \(.), which is one less than \(.+1)""#);
    assert_eq!(result, vec![r#"""The input was 42, which is one less than 43"""#]);
}

//#[test]
fn conditionals() {
    let result = process(r#"2 | if . == 0 then "zero" elif . == 1 then "one" else "many" end"#);
    assert_eq!(result, vec![r#""many""#]);
}

// implemented: array construction while iterating is covered by
// tests/jq_semantics.rs (array_construction_builds_one_array_per_element);
// quoted field access `."a"` remains unimplemented
//#[test]
fn quoted_field_access() {
    let result = process(r#"[{"a": 1}, {"a": 2}][] | [."a"]"#);
    assert_eq!(result, vec!["[1]", "[2]"]);
}
