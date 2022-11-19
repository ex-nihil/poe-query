
#[cfg(test)]
mod subtraction {
    use crate::tests::test_util::process;

    #[test]
    fn numbers() {
        let result = process("10 - 5");
        assert_eq!(result[0], "5");
    }

    #[test]
    fn arrays() {
        let result = process(r#"["xml", "yaml", "json"] - ["xml", "yaml"]"#);
        assert_eq!(result, vec![r#"["json"]"#]);
    }
}