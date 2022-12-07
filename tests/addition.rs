mod common;
use common::process;

#[test]
fn numbers() {
    let result = process("1 + 1");
    assert_eq!(result[0], "2");
}

#[test]
fn strings() {
    let result = process(r#""1" + "1""#);
    assert_eq!(result, vec![r#""11""#]);
}

#[test]
fn arrays() {
    let result = process("[0] + [1]");
    assert_eq!(result, vec!["[0,1]"]);
}

#[test]
fn objects() {
    let result = process("{foo: 0, bar: 1} + {foo: 1, baz: 1}");
    assert_eq!(result, vec![r#"{"bar":1,"foo":1,"baz":1}"#]);
}
