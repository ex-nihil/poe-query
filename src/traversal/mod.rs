use crate::{DatReader, Term};

pub mod value;
mod traverse;
mod utils;

pub trait TermsProcessor {
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
