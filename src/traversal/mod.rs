use crate::{DatReader, Term};

pub mod value;
mod traverse;
mod utils;

// TODO:
//  Consider splitting QueryProcessor trait into multiple traits that each define a specific behavior or capability, such as DataAccessor, DataTransformer, or DataAggregator.
pub trait QueryProcessor {
    fn process(&self, terms: &[Term]) -> value::Value;
}

/** Immutable data during traversal */
#[derive(Default)]
pub struct StaticContext<'a> {
    store: Option<&'a DatReader<'a>>,
}

impl<'a> StaticContext<'a> {
    pub fn new(reader: &'a DatReader<'a>) -> Self {
        StaticContext {
            store: Some(reader)
        }
    }
}
