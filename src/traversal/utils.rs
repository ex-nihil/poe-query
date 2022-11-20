use crate::Value;

pub fn iterate<F>(value: Value, mut action: F) -> Value
    where
        F: FnMut(Value) -> Option<Value> + Send + Sync,
{
    match value {
        Value::Iterator(elements) => {
            let mut list = Vec::new();
            for e in elements {
                if let Some(v) = action(e) {
                    list.push(v);
                }
            }
            Value::List(list)
        },
        v => action(v).expect("non-iterable must return something"),
    }
}

pub fn reduce<F>(initial: Value, action: &mut F) -> Value
    where
        F: FnMut(Value, Value) -> Value,
{
    match initial {
        Value::Iterator(elements) => {
            elements.into_iter().reduce(|accum, item| {
                action(accum, item)
            }).unwrap_or(Value::Empty)
        }
        _ => {
            action(Value::Empty, initial)
        }
    }
}