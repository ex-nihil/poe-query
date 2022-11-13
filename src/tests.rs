
#[cfg(test)]
mod tests {
    use crate::{query, StaticContext, TermsProcessor, Value};

    #[test]
    fn create_array() {
        let result = process("[]");
        assert_eq!(result, vec!["[]"]);

        let result = process("[0, 1, 2]");
        assert_eq!(result, vec!["[0,1,2]"]);
    }

    #[test]
    fn create_object() {
        let result = process("{ foo: 1, bar: 2}");
        assert_eq!(result, vec![r#"{"foo":1,"bar":2}"#]);
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
    fn index_negative() {
        let result = process("[5, 6, 7][-1]");
        assert_eq!(result[0], "7");

        let result = process("[5, 6, 7][-0]");
        assert_eq!(result[0], "5");
    }

    #[test]
    fn add_values() {
        let result = process("1 + 1");
        assert_eq!(result[0], "2");
    }

    #[test]
    fn add_strings() {
        let result = process(r#""1" + "1""#);
        assert_eq!(result, vec![r#""11""#]);
    }

    #[test]
    fn add_arrays() {
        let result = process("[0] + [1]");
        assert_eq!(result, vec!["[0,1]"]);
    }

    #[test]
    fn select() {
        let result = process("[0, 1, 2] | map(select(. >= 2))");
        assert_eq!(result[0], "[2]");
    }

    #[test]
    fn field() {
        let result = process("{ foo: 1, bar: 2} | .foo");
        assert_eq!(result, vec!["1"]);

        let result = process("{ foo: 1, bar: 2}.bar");
        assert_eq!(result, vec!["2"]);
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