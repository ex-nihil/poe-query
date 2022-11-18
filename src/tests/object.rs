#[cfg(test)]
mod object {
    use crate::tests::test_util::process;

    #[test]
    fn create_object_select_kv() {
        let result = process(r#"{foo: 0, bar: 1} | {bar, baz: 2}"#);
        assert_eq!(result, vec![r#"{"bar":1,"baz":2}"#]);
    }

    #[test]
    fn create_object() {
        let result = process("{ foo: 1, bar: 2}");
        assert_eq!(result, vec![r#"{"foo":1,"bar":2}"#]);
    }

}