#[cfg(test)]
mod string {
    use crate::tests::test_util::process;

    #[test]
    fn slice_string_negative_indices() {
        let result = process("\"abcdefg\" | .[-3:-1]");
        assert_eq!(result[0], "\"ef\"");
    }

    #[test]
    fn index_string() {
        // this is not supported by jq, drop if it conflicts with something else
        let result = process(r#""abc" | .[1]"#);
        assert_eq!(result[0], r#""b""#);

        let result = process(r#""åäö" | .[1]"#);
        assert_eq!(result[0], r#""ä""#);
    }

    #[test]
    fn string_length() {
        // unicode characters, not bytes
        let result = process("\"abcåäö\" | length");
        assert_eq!(result, vec!["6"]);
    }
}