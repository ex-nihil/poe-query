
#[cfg(test)]
mod tests {
    use crate::tests::test_util::process;

    #[test]
    fn create_array_empty() {
        // TODO: clashes with 'iterate'
        let result = process("[]");
        assert_eq!(result, vec!["[]"]);
    }

    #[test]
    fn create_array() {
        let result = process("[0, 1, 2]");
        assert_eq!(result, vec!["[0,1,2]"]);
    }

    #[test]
    fn create_object() {
        let result = process("{ foo: 1, bar: 2}");
        assert_eq!(result, vec![r#"{"foo":1,"bar":2}"#]);
    }

    #[test]
    fn create_object_select_kv() {
        let result = process(r#"{foo: 0, bar: 1} | {bar, baz: 2}"#);
        assert_eq!(result, vec![r#"{bar:1,baz:2}"#]);
    }

    #[test]
    fn array_length() {
        let result = process("[0,1,2,3] | length");
        assert_eq!(result, vec!["4"]);
    }

    #[test]
    fn object_keys() {
        let result = process(r#"{"abc": 1, "abcd": 2, "Foo": 3} | keys"#);
        assert_eq!(result, vec![r#"["Foo","abc","abcd"]"#]);
    }

    #[test]
    fn multiple_queries() {
        let result = process(r#"[1,2,3] | .[1], .[0]"#);
        assert_eq!(result, vec!["2", "1"]);
    }

    #[test]
    fn conditionals() {
        let result = process(r#"2 | if . == 0 then "zero" elif . == 1 then "one" else "many" end"#);
        assert_eq!(result, vec![r#""many""#]);
    }

    #[test]
    fn string_interpolation() {
        let result = process(r#"42 | "The input was \(.), which is one less than \(.+1)""#);
        assert_eq!(result, vec![r#"""The input was 42, which is one less than 43"""#]);
    }

    #[test]
    fn iterate() {
        let result = process("[0, 1, 2][]");
        assert_eq!(result, vec!["0", "1", "2"]);
    }

    #[test]
    fn index() {
        let result = process("[5, 6, 7][1]");
        assert_eq!(result[0], "6");
    }

    #[test]
    fn index_slice() {
        let result = process("[5, 6, 7, 8][1:3]");
        assert_eq!(result[0], "[6,7]");
    }

    #[test]
    fn index_string() {
        // this is not supported by jq, drop if it conflicts with something else
        let result = process(r#""abc" | .[1]"#);
        assert_eq!(result[0], r#""b""#);
    }

    #[test]
    fn index_negative() {
        let result = process("[5, 6, 7][-1]");
        assert_eq!(result[0], "7");

        let result = process("[5, 6, 7][-0]");
        assert_eq!(result[0], "5");
    }

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

}