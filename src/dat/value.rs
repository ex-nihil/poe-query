use std::ops;
use std::process;
use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};
use log::*;

#[derive(Debug, Clone, PartialOrd, PartialEq)]
pub enum Value {
    Str(String),
    Byte(u8),
    U64(u64),
    I64(i64),
    List(Vec<Value>),
    Iterator(Vec<Value>),
    KeyValue(Box<Value>, Box<Value>),
    Object(Box<Value>), // TODO: make this a map instead?
    Bool(bool),
    Empty,
}

impl ops::Add<Value> for Value {
    type Output = Value;

    fn add(self, _rhs: Value) -> Value {
        let result = match self {
            Value::Str(lhs) => {
                if let Value::Str(rhs) = _rhs {
                    Value::Str(format!("{}{}", lhs, rhs))
                } else {
                    panic!("operations requires both sides to be of same type");
                }
            }
            Value::U64(lhs) => {
                if let Value::U64(rhs) = _rhs {
                    Value::U64(lhs + rhs)
                } else {
                    panic!("operations requires both sides to be of same type");
                }
            }
            Value::I64(lhs) => {
                if let Value::I64(rhs) = _rhs {
                    Value::I64(lhs + rhs)
                } else {
                    panic!("operations requires both sides to be of same type");
                }
            }
            Value::Byte(lhs) => {
                if let Value::Byte(rhs) = _rhs {
                    Value::Byte(lhs + rhs)
                } else {
                    panic!("operations requires both sides to be of same type");
                }
            }
            Value::List(lhs) => {
                if let Value::List(rhs) = _rhs {
                    Value::List([&lhs[..], &rhs[..]].concat())
                } else {
                    panic!("operations requires both sides to be of same type");
                }
            }
            Value::Object(boxed_lhs) => {
                // TODO: this feels atrocious, search for a better way 
                let rhs = match _rhs {
                    Value::Object(boxed_rhs) => {
                        match *boxed_rhs {
                            Value::List(rhs) => rhs,
                            Value::KeyValue(_, _) => vec![*boxed_rhs],
                            Value::Empty => vec![],
                            _ => panic!("operations requires both sides to be of same type"),
                        }
                    }
                    _ => panic!("operations requires both sides to be of same type"),
                };

                let result = match *boxed_lhs {
                    Value::List(lhs) => Value::List([&lhs[..], &rhs[..]].concat()),
                    Value::KeyValue(_, _) => Value::List([&[*boxed_lhs], &rhs[..]].concat()),
                    Value::Empty => Value::List(rhs),
                    _ => panic!("operations requires both sides to be of same type"),
                };
                Value::Object(Box::new(result))
            }
            Value::Iterator(_) => {
                panic!("addition of iterators not yet implemented");
            }
            _ => panic!("type does not support add operation"),
        };

        result
    }
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
                            Value::KeyValue(k, v) => {
                                map.serialize_entry(&*k, &*v)?
                            },
                            _ => panic!("object contained an unexpected value"),
                        }
                    }
                    map.end()
                } else if let Value::KeyValue(k, v) = *content.clone() {
                    let mut map = serializer.serialize_map(Some(1))?;
                    map.serialize_entry(&*k, &*v)?;
                    map.end()
                } else if let Value::Empty = *content.clone() {
                    let map = serializer.serialize_map(Some(1))?;
                    map.end()
                } else {
                    error!("object contained an unexpected value: {:?}", content);
                    process::exit(-1);
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
