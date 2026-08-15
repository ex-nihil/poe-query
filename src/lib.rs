#[macro_use]
extern crate pest_derive;
extern crate core;


pub mod dat;
pub mod error;
pub mod introspect;
pub mod query;
pub mod traversal;

pub use crate::dat::{DatReader, DatStoreImpl};
pub use crate::error::QueryError;
pub use crate::query::{parse_query, Term};
pub use crate::traversal::{QueryProcessor, SharedCache, StaticContext};
pub use crate::traversal::value::Value;