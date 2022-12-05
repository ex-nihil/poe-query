use log::*;
use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};
use std::{fmt, ops};
use std::cmp::Ordering;
use std::fmt::Formatter;
use std::ops::Deref;
use std::process;

#[derive(Debug, Clone)]
pub enum Value {
    Str(String),
    Byte(u8),
    U64(u64),
    I64(i64),
    F32(f32),
    List(Vec<Value>),
    Iterator(Vec<Value>),
    KeyValue(Box<Value>, Box<Value>),
    Object(Box<Value>), // Make this a map instead? Comparisons might be a problem.
    Bool(bool),
    Empty,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Value::Str(_) => write!(f, "String"),
            Value::Byte(_) => write!(f, "Byte"),
            Value::U64(_) => write!(f, "Int"),
            Value::I64(_) => write!(f, "Int"),
            Value::F32(_) => write!(f, "Float"),
            Value::List(list) => write!(f, "List(length = {})", list.len()),
            Value::Iterator(_) => write!(f, "Iterator"),
            Value::KeyValue(_, _) => write!(f, "KeyValue"),
            Value::Object(_) => write!(f, "Object"),
            Value::Bool(_) => write!(f, "Bool"),
            Value::Empty => write!(f, "Empty"),
        }
    }
}

impl ops::Add<Value> for Value {
    type Output = Value;

    fn add(self, rhs: Value) -> Value {
        use Value::*;
        match (self, rhs) {
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

                // strip out keys that should be overwritten
                let lhs_content: Vec<Value> = lhs_content.into_iter().filter(|e| {
                    match e {
                        KeyValue(key, _) => {
                            !rhs_content.iter().filter_map(|x| x.key()).any(|x| x == key.as_ref())
                        },
                        _ => true
                    }
                }).collect();

                Value::Object(Box::new(Value::List(
                    [&lhs_content[..], &rhs_content[..]].concat(),
                )))
            }
            (lhs, rhs) => {
                error!("Operation not supported: {} + {}", lhs, rhs);
                process::exit(-1);
            }
        }
    }
}

impl Value {
    fn key(&self) -> Option<&Value> {
        match self {
            Value::KeyValue(key, _) => Some(key),
            _ => None
        }
    }
}

impl ops::Sub<Value> for Value {
    type Output = Value;

    fn sub(self, rhs: Value) -> Value {
        use Value::*;
        match (self, rhs) {
            (Empty, Empty) => Empty,
            (U64(lhs), U64(rhs)) => U64(lhs - rhs),
            (I64(lhs), I64(rhs)) => I64(lhs - rhs),
            (Byte(lhs), Byte(rhs)) => Byte(lhs - rhs),
            (List(lhs), List(rhs)) => {
                List(lhs.into_iter().filter(|e| !rhs.contains(e)).collect())
            },
            (lhs, rhs) => {
                error!("Subtraction not supported: {} - {}", lhs, rhs);
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
                    map.serialize_entry(k, v)?;
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
            Value::F32(value) => serializer.serialize_f32(*value),
            Value::Bool(value) => serializer.serialize_bool(*value),
            Value::Empty => serializer.serialize_unit(),
        }
    }
}

impl PartialEq<Value> for Value {
    fn eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::U64(lhs), Value::I64(rhs)) => *lhs as i128 == *rhs as i128,
            (Value::I64(lhs), Value::U64(rhs)) => *lhs as i128 == *rhs as i128,

            (Value::Str(lhs), Value::Str(rhs)) => lhs == rhs,
            (Value::Byte(lhs), Value::Byte(rhs)) => lhs == rhs,
            (Value::U64(lhs), Value::U64(rhs)) => lhs == rhs,
            (Value::I64(lhs), Value::I64(rhs)) => lhs == rhs,
            (Value::F32(lhs), Value::F32(rhs)) => lhs == rhs,
            (Value::List(lhs), Value::List(rhs)) => lhs == rhs,
            (Value::Iterator(lhs), Value::Iterator(rhs)) => lhs == rhs,
            (Value::KeyValue(lhs_lhs, lhs_rhs), Value::KeyValue(rhs_lhs, rhs_rhs)) => {
                lhs_lhs == rhs_lhs && lhs_rhs == rhs_rhs
            },
            (Value::Object(lhs), Value::Object(rhs)) => lhs == rhs,
            (Value::Bool(lhs), Value::Bool(rhs)) => lhs == rhs,
            (Value::Empty, Value::Empty) => true,
            _ => false
        }
    }
}

impl PartialOrd<Value> for Value {
    fn partial_cmp(&self, other: &Value) -> Option<Ordering> {
        match (self, other) {
            (Value::U64(lhs), Value::I64(rhs)) => {
                (*lhs as i128).partial_cmp(&(*rhs as i128))
            },
            (Value::I64(lhs), Value::U64(rhs)) => {
                (*lhs as i128).partial_cmp(&(*rhs as i128))
            },
            (Value::U64(lhs), Value::U64(rhs)) => lhs.partial_cmp(rhs),
            (Value::I64(lhs), Value::I64(rhs)) => lhs.partial_cmp(rhs),
            (Value::F32(lhs), Value::F32(rhs)) => lhs.partial_cmp(rhs),

            (lhs, rhs) if lhs == rhs => Some(Ordering::Equal),
            _ => None
        }
    }
}