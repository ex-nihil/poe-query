
#[cfg(test)]
mod tests {
    use crate::tests::test_util::process;

    #[test]
    fn string_length() {
        // unicode characters, not bytes
        let result = process("\"abcåäö\" | length");
        assert_eq!(result, vec!["6"]);
    }

    #[test]
    fn multiple_queries() {
        let result = process(r#"[1,2,3] | .[1], .[0]"#);
        assert_eq!(result, vec!["2", "1"]);
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
    fn slice() {
        let result = process("[5, 6, 7, 8][1:3]");
        assert_eq!(result[0], "[6,7]");
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