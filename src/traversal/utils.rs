use crate::error::QueryError;
use crate::Value;

pub fn iterate<F>(value: Value, mut action: F) -> Result<Value, QueryError>
    where
        F: FnMut(Value) -> Result<Option<Value>, QueryError>,
{
    match value {
        Value::Iterator(elements) => {
            let mut list = Vec::with_capacity(elements.len());
            for element in elements {
                if let Some(value) = action(element)? {
                    list.push(value);
                }
            }
            Ok(Value::List(list))
        }
        _ => Ok(action(value)?.unwrap_or(Value::Empty)),
    }
}

pub fn reduce<F>(initial: Value, mut action: F) -> Result<Value, QueryError>
    where
        F: FnMut(Value, Value) -> Result<Value, QueryError>,
{
    match initial {
        Value::Iterator(elements) => {
            let mut elements = elements.into_iter();
            let Some(mut accumulator) = elements.next() else {
                return Ok(Value::Empty);
            };
            for element in elements {
                accumulator = action(accumulator, element)?;
            }
            Ok(accumulator)
        }
        _ => action(Value::Empty, initial),
    }
}
