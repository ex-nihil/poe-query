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

//#[test]
fn multiple_queries() {
    let result = process(r#"[1,2,3] | .[1], .[0]"#);
    assert_eq!(result, vec!["2", "1"]);
}