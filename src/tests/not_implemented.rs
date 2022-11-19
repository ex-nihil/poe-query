#[cfg(test)]
mod not_implemented {
    use crate::tests::test_util::process;

    #[test]
    fn string_interpolation() {
        let result = process(r#"42 | "The input was \(.), which is one less than \(.+1)""#);
        assert_eq!(result, vec![r#"""The input was 42, which is one less than 43"""#]);
    }

    #[test]
    fn conditionals() {
        let result = process(r#"2 | if . == 0 then "zero" elif . == 1 then "one" else "many" end"#);
        assert_eq!(result, vec![r#""many""#]);
    }
}