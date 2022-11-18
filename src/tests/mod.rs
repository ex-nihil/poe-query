mod addition;
mod subtraction;
mod tests;



#[cfg(test)]
pub mod test_util {
    use crate::{query, StaticContext, TermsProcessor, Value};

    pub fn process(input: &str) -> Vec<String> {
        let terms = query::parse(input);
        println!("Input: {}", input);
        println!("Terms: {:?}", terms);

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