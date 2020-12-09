use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};

#[derive(Debug, Clone, PartialOrd, PartialEq)]
pub enum Value {
    Str(String),
    Byte(u8),
    U64(u64),
    I64(i64),
    List(Vec<Value>),
    Iterator(Vec<Value>),
    KeyValue(String, Box<Value>),
    Object(Box<Value>),
    Bool(bool),
    Empty,
}

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Value::Object(content) => {
                if let Value::List(list) = *content.clone() {
                    let mut map = serializer.serialize_map(Some(list.len()))?;
                    for value in list {
                        match value {
                            Value::KeyValue(k, v) => map.serialize_entry(k.as_str(), &*v)?,
                            _ => panic!("object contained an unexpected value"),
                        }
                    }
                    map.end()
                } else if let Value::KeyValue(k, v) = *content.clone() {
                    let mut map = serializer.serialize_map(Some(1))?;
                    map.serialize_entry(k.as_str(), &*v)?;
                    map.end()
                } else {
                    panic!("object contained an unexpected value");
                }
            }
            Value::List(list) => {
                let mut seq = serializer.serialize_seq(Some(list.len()))?;
                for value in list {
                    seq.serialize_element(value)?;
                }
                seq.end()
            }
            Value::Iterator(list) => {
                let mut seq = serializer.serialize_seq(Some(list.len()))?;
                for value in list {
                    seq.serialize_element(value)?;
                }
                seq.end()
            }
            Value::Str(text) => serializer.serialize_str(text),
            Value::KeyValue(_, value) => value.serialize(serializer),
            Value::Byte(value) => serializer.serialize_u8(*value),
            Value::U64(value) => serializer.serialize_u64(*value),
            Value::I64(value) => serializer.serialize_i64(*value),
            Value::Bool(value) => serializer.serialize_bool(*value),
            Value::Empty => serializer.serialize_unit(),
        }
    }
}
