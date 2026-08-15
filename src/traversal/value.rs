use serde::ser::{Error as SerdeError, Serialize, SerializeMap, SerializeSeq, Serializer};
use std::fmt;
use std::cmp::Ordering;
use std::fmt::Formatter;
use std::ops::Deref;
use std::rc::Rc;

use crate::error::QueryError;

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
    /// Lazy handle to a table row: (file name, row index). Fields are read
    /// on demand; any Row remaining in the final result is expanded to a
    /// full object before serialization.
    Row(Rc<str>, u64),
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
            Value::Row(_, _) => write!(f, "Row"),
            Value::Empty => write!(f, "Empty"),
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

    pub fn try_add(self, rhs: Value) -> Result<Value, QueryError> {
        use Value::*;
        match (self, rhs) {
            (Empty, Empty) => Ok(Empty),
            (Str(lhs), Str(rhs)) => Ok(Str(format!("{}{}", lhs, rhs))),
            (U64(lhs), U64(rhs)) => Ok(U64(lhs + rhs)),
            (I64(lhs), I64(rhs)) => Ok(I64(lhs + rhs)),
            (Byte(lhs), Byte(rhs)) => Ok(Byte(lhs + rhs)),
            (List(lhs), List(rhs)) => Ok(List([&lhs[..], &rhs[..]].concat())),
            (Iterator(lhs), Iterator(rhs)) => Ok(Iterator([&lhs[..], &rhs[..]].concat())),
            (Object(lhs), Object(rhs)) => {
                let lhs_content = object_content(*lhs)?;
                let rhs_content = object_content(*rhs)?;

                // strip out keys that should be overwritten
                let lhs_content: Vec<Value> = lhs_content.into_iter().filter(|e| {
                    match e {
                        KeyValue(key, _) => {
                            !rhs_content.iter().filter_map(|x| x.key()).any(|x| x == key.as_ref())
                        },
                        _ => true
                    }
                }).collect();

                Ok(Value::Object(Box::new(Value::List(
                    [&lhs_content[..], &rhs_content[..]].concat(),
                ))))
            }
            (lhs, rhs) => Err(QueryError::type_error("addition", format!("{} + {}", lhs, rhs))),
        }
    }

    pub fn try_sub(self, rhs: Value) -> Result<Value, QueryError> {
        use Value::*;
        match (self, rhs) {
            (Empty, Empty) => Ok(Empty),
            (U64(lhs), U64(rhs)) => Ok(U64(lhs - rhs)),
            (I64(lhs), I64(rhs)) => Ok(I64(lhs - rhs)),
            (Byte(lhs), Byte(rhs)) => Ok(Byte(lhs - rhs)),
            (List(lhs), List(rhs)) => {
                Ok(List(lhs.into_iter().filter(|e| !rhs.contains(e)).collect()))
            },
            (lhs, rhs) => Err(QueryError::type_error("subtraction", format!("{} - {}", lhs, rhs))),
        }
    }
}

fn object_content(content: Value) -> Result<Vec<Value>, QueryError> {
    match content {
        Value::List(list) => Ok(list),
        Value::Iterator(list) => Ok(list),
        kv @ Value::KeyValue(_, _) => Ok(vec![kv]),
        Value::Empty => Ok(vec![]),
        unexpected => Err(QueryError::internal(format!("object contained unexpected type {}", unexpected))),
    }
}

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Value::Object(content) => match content.deref() {
                Value::List(list) | Value::Iterator(list) => {
                    let mut map = serializer.serialize_map(Some(list.len()))?;
                    for value in list {
                        match value {
                            Value::KeyValue(k, v) => {
                                map.serialize_entry(k.as_ref(), v.as_ref())?
                            }
                            Value::Empty => {}
                            _ => {
                                return Err(S::Error::custom(format!("object contained an unexpected value: {:?}", value)));
                            }
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
                    return Err(S::Error::custom(format!("object contained an unexpected value: {:?}", content)));
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
            // rows are expanded before serialization; fall back to the row index
            Value::Row(_, row) => serializer.serialize_u64(*row),
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
            (Value::Row(lhs_file, lhs_row), Value::Row(rhs_file, rhs_row)) => {
                lhs_file == rhs_file && lhs_row == rhs_row
            },
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
