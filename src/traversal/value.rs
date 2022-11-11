use log::*;
use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};
use std::ops;
use std::ops::Deref;
use std::process;

#[derive(Debug, Clone, PartialOrd, PartialEq)]
pub enum Value {
    Str(String),
    Byte(u8),
    U64(u64),
    I64(i64),
    List(Vec<Value>),
    Iterator(Vec<Value>),
    KeyValue(Box<Value>, Box<Value>),
    Object(Box<Value>), // Make this a map instead? Comparisons might be a problem.
    Bool(bool),
    Empty,
}

impl ops::Add<Value> for Value {
    type Output = Value;

    fn add(self, _rhs: Value) -> Value {
        use Value::*;
        match (self, _rhs) {
            (Empty, Empty) => Empty,
            (Str(lhs), Str(rhs)) => Str(format!("{}{}", lhs, rhs)),
            (U64(lhs), U64(rhs)) => U64(lhs + rhs),
            (I64(lhs), I64(rhs)) => I64(lhs + rhs),
            (Byte(lhs), Byte(rhs)) => Byte(lhs + rhs),
            (List(lhs), List(rhs)) => List([&lhs[..], &rhs[..]].concat()),
            (Iterator(lhs), Iterator(rhs)) => Iterator([&lhs[..], &rhs[..]].concat()),
            (Object(lhs), Object(rhs)) => {
                let lhs_content = match *lhs {
                    Value::List(list) => list,
                    Value::KeyValue(_, _) => vec![*lhs],
                    Value::Empty => vec![],
                    _ => panic!("object contained unknown type"),
                };
                let rhs_content = match *rhs {
                    Value::List(list) => list,
                    Value::KeyValue(_, _) => vec![*rhs],
                    Value::Empty => vec![],
                    _ => panic!("object contained unknown type"),
                };
                Value::Object(Box::new(Value::List(
                    [&lhs_content[..], &rhs_content[..]].concat(),
                )))
            }
            (lhs, rhs) => {
                error!("Operation not supported: {:?} + {:?}", lhs, rhs);
                process::exit(-1);
            }
        }
    }
}

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        //warn!("{:?}", self);
        match self {
            Value::Object(content) => match content.deref() {
                Value::List(list) => {
                    let mut map = serializer.serialize_map(Some(list.len()))?;
                    for value in list {
                        if let Value::KeyValue(k, v) = value {
                            map.serialize_entry(k.as_ref(), v.as_ref())?
                        } else {
                            error!("object contained an unexpected value: {:?}", value);
                            process::exit(-1);
                        }
                    }
                    map.end()
                }
                Value::KeyValue(k, v) => {
                    let mut map = serializer.serialize_map(Some(1))?;
                    map.serialize_entry(&*k, &*v)?;
                    map.end()
                }
                Value::Empty => serializer.serialize_map(Some(0))?.end(),
                _ => {
                    error!("object contained an unexpected value: {:?}", content);
                    process::exit(-1);
                }
            },
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
