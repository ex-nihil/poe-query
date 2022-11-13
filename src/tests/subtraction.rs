
#[cfg(test)]
mod subtraction {
    use crate::{query, StaticContext, TermsProcessor, Value};
    /**
    As well as normal arithmetic subtraction on numbers, the - operator can be used on arrays to remove all occurrences of the second array's elements from the first array.
     */

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


    fn process(input: &str) -> Vec<String> {
        let terms = query::parse(input);

        let value = StaticContext::default()
            .process_terms(&terms);

        match value {
            Value::Iterator(items) => {
                items.iter().map(|item| {
                    serde_json::to_string(item).expect("seralized")
                }).collect()
            }
            _ => vec![serde_json::to_string(&value).expect("serialized")]
        }
    }
}