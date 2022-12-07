use crate::Value;

pub fn iterate<F>(value: Value, mut action: F) -> Value
    where
        F: FnMut(Value) -> Option<Value> + Send + Sync,
{
    match value {
        Value::Iterator(elements) => {
            let list = elements.into_iter()
                .filter_map(action)
                .collect();
            Value::List(list)
        }
        _ => action(value).expect("non-iterable must return something"),
    }
}

pub fn reduce<F>(initial: Value, mut action: F) -> Value
    where
        F: FnMut(Value, Value) -> Value,
{
    match initial {
        Value::Iterator(elements) =>
            elements.into_iter().reduce(action).unwrap_or(Value::Empty),
        _ => action(Value::Empty, initial)
    }
}