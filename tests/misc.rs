mod common;
use common::process;

#[test]
fn map() {
    let result = process("[0, 1, 2] | map(.+1)");
    assert_eq!(result[0], "[1,2,3]");
}

#[test]
fn select() {
    let result = process("[0, 1, 2] | select(true)");
    assert_eq!(result[0], "[0,1,2]");

    let result = process("[0, 1, 2] | select(false)");
    assert_eq!(result[0], "[]");

    let result = process("[0, 1, 2] | select(. >= 2)");
    assert_eq!(result[0], "[2]");
}

#[test]
fn field() {
    let result = process("{ foo: 1, bar: 2} | .foo");
    assert_eq!(result, vec!["1"]);

    let result = process("{ foo: 1, bar: 2}.bar");
    assert_eq!(result, vec!["2"]);
}

