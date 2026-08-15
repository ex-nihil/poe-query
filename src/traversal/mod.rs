use crate::Term;
use crate::dat::DatStoreImpl;
use crate::error::QueryError;

pub mod value;
mod traverse;
mod utils;

// TODO:
//  Consider splitting QueryProcessor trait into multiple traits that each define a specific behavior or capability, such as DataAccessor, DataTransformer, or DataAggregator.
pub trait QueryProcessor {
    fn process(&self, terms: &[Term]) -> Result<value::Value, QueryError>;
}

/** Immutable data during traversal */
#[derive(Default)]
pub struct StaticContext<'a> {
    store: Option<&'a dyn DatStoreImpl<'a>>,
}

impl<'a> StaticContext<'a> {
    pub fn new(store: &'a dyn DatStoreImpl<'a>) -> Self {
        StaticContext {
            store: Some(store)
        }
    }
}
