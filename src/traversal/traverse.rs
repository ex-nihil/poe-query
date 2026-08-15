use std::cmp::min;
use std::collections::HashMap;
use std::rc::Rc;

use log::*;

use crate::Term;
use crate::dat::file::DatFile;
use crate::dat::DatStoreImpl;
use crate::dat::specification::{FieldSpecImpl, FileSpec, FileSpecImpl};
use crate::error::{closest_name, QueryError};
use crate::query::{Compare, Operation};
use crate::traversal::{StaticContext, QueryProcessor};
use crate::traversal::utils::{iterate, reduce};

use super::value::Value;

/** entry point */
impl QueryProcessor for StaticContext<'_> {
    fn process(&self, terms: &[Term]) -> Result<Value, QueryError> {
        let mut cache = SharedCache::default();
        self.process_with_cache(&mut cache, terms)
    }
}

impl<'a> StaticContext<'a> {
    /// Evaluate a query reusing a caller-owned cache, so decoded data files
    /// survive across queries in a long-lived process. Query-local variables
    /// are cleared on entry so nothing leaks between queries.
    pub fn process_with_cache(&self, cache: &mut SharedCache, terms: &[Term]) -> Result<Value, QueryError> {
        cache.variables.clear();
        let result = self.traverse(&mut TraversalContext::default(), cache, terms)?;
        self.materialize(cache, result)
    }
    /// Expand any remaining lazy row handles into full objects so the
    /// serialized output is identical to the eager representation.
    fn materialize(&self, cache: &mut SharedCache, value: Value) -> Result<Value, QueryError> {
        match value {
            Value::Row(file, row) => self.materialize_row(cache, &file, row),
            Value::List(items) => {
                let mut materialized = Vec::with_capacity(items.len());
                for item in items {
                    materialized.push(self.materialize(cache, item)?);
                }
                Ok(Value::List(materialized))
            }
            Value::Iterator(items) => {
                let mut materialized = Vec::with_capacity(items.len());
                for item in items {
                    materialized.push(self.materialize(cache, item)?);
                }
                Ok(Value::Iterator(materialized))
            }
            Value::Object(inner) => Ok(Value::Object(Box::new(self.materialize(cache, *inner)?))),
            Value::KeyValue(key, value) => Ok(Value::KeyValue(
                Box::new(self.materialize(cache, *key)?),
                Box::new(self.materialize(cache, *value)?),
            )),
            other => Ok(other),
        }
    }

    /// Load a table's data file on first use and keep it for the rest of the traversal.
    fn cached_file<'c>(&self, cache: &'c mut SharedCache, file_name: &str) -> Result<&'c DatFile, QueryError> {
        if !cache.files.contains_key(file_name) {
            let store = self.store
                .ok_or_else(|| QueryError::internal(format!("no data store loaded, cannot read table '{}'", file_name)))?;
            let file = store.file_by_filename(file_name)?;
            cache.warnings.extend(file.warnings.iter().cloned());
            cache.files.insert(file_name.to_string(), file);
        }
        Ok(cache.files.get(file_name).unwrap())
    }

    fn materialize_row(&self, cache: &mut SharedCache, file_name: &str, row: u64) -> Result<Value, QueryError> {
        let store = self.store
            .ok_or_else(|| QueryError::internal("no data store loaded"))?;
        let spec = store.spec(file_name)
            .ok_or_else(|| QueryError::internal(format!("no specification for table '{}'", file_name)))?;
        let file = self.cached_file(cache, file_name)?;

        let mut kv_list = Vec::with_capacity(spec.file_fields.len());
        for field in &spec.file_fields {
            kv_list.push(Value::KeyValue(
                Box::new(Value::Str(field.field_name.clone())),
                Box::new(file.read_field(row, field)?),
            ));
        }
        Ok(Value::Object(Box::new(Value::List(kv_list))))
    }

    /// Read a single column of a single row, loading the file on first use.
    fn read_row_field(&self, cache: &mut SharedCache, file_name: &str, row: u64, field_name: &str) -> Result<Value, QueryError> {
        let Some(store) = self.store else { return Ok(Value::Empty) };
        let spec = store.spec(file_name)
            .ok_or_else(|| QueryError::internal(format!("no specification for table '{}'", file_name)))?;
        let Some(field) = spec.field(field_name) else {
            let suggestion = closest_name(field_name, spec.file_fields.iter().map(|f| f.field_name.as_str()));
            return Err(QueryError::UnknownColumn {
                table: file_name.to_string(),
                column: field_name.to_string(),
                suggestion,
            });
        };
        let file = self.cached_file(cache, file_name)?;
        file.read_field(row, field)
    }
}

/** Shared mutable data during traversal */
#[derive(Default)]
pub struct SharedCache {
    variables: HashMap<String, Value>,
    files: HashMap<String, DatFile>,
    warnings: Vec<String>,
}

impl SharedCache {
    /// Drain schema drift warnings collected while loading data files, so a
    /// long-lived process can report them alongside the query result.
    pub fn take_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.warnings)
    }
}

/** Local mutable data during traversal */
#[derive(Debug, Clone, Default)]
struct TraversalContext {
    current_field: Option<String>,
    current_file: Option<String>,
    identity: Option<Value>,
    /// identity currently holds the synthetic root table listing produced by
    /// a bare `.`, so a following name lookup is a table lookup
    root_listing: bool,
}

trait DataTraverser<'a> {
    fn traverse(&self, context: &mut TraversalContext, cache: &mut SharedCache, parsed_terms: &[Term]) -> Result<Value, QueryError>;
    fn traverse_term(&self, context: &mut TraversalContext, cache: &mut SharedCache, term: &Term) -> Result<Value, QueryError>;
    fn traverse_terms_inner(&self, context: &mut TraversalContext, cache: &mut SharedCache, terms: &[Term]) -> Result<Option<Value>, QueryError>;

    fn child(&self, context: &mut TraversalContext, cache: &mut SharedCache, name: &str) -> Result<(), QueryError>;
    fn index(&self, context: &mut TraversalContext, index: usize);
    fn index_reverse(&self, context: &mut TraversalContext, index: usize);
    fn slice(&self, context: &mut TraversalContext, from: i64, to: i64) -> Result<(), QueryError>;
    fn to_iterable(&self, context: &mut TraversalContext, cache: &mut SharedCache) -> Result<Value, QueryError>;
    fn value(&self, context: &mut TraversalContext, cache: &mut SharedCache) -> Result<Value, QueryError>;
    fn identity(&self, context: &mut TraversalContext) -> Value;

    fn enter_foreign(&self, context: &mut TraversalContext, cache: &mut SharedCache) -> Result<(), QueryError>;
    fn rows_from(&self, file: &str, indices: &[u64]) -> Result<Value, QueryError>;
}

impl<'a> DataTraverser<'a> for StaticContext<'a> {
    fn traverse(&self, context: &mut TraversalContext, cache: &mut SharedCache, parsed_terms: &[Term]) -> Result<Value, QueryError> {
        let values: Vec<Value> = if parsed_terms.contains(&Term::PipeOperator) {
            let mut ident = context.identity();
            for terms in parsed_terms.split(|term| matches!(term, Term::PipeOperator)) {
                let mut c = context.clone_value(Some(ident));
                ident = self.traverse(&mut c, cache, terms)?;
                context.current_file = c.current_file;
                context.current_field = c.current_field;
            }
            vec![ident]
        } else if parsed_terms.contains(&Term::CommaSeparator) {
            let mut values = Vec::new();
            for terms in parsed_terms.split(|term| matches!(term, Term::CommaSeparator)) {
                values.push(self.traverse(&mut context.clone(), cache, terms)?);
            }
            values
        } else {
            vec![self
                .traverse_terms_inner(context, cache, parsed_terms)?
                .unwrap_or(Value::Empty)]
        };

        context.identity = match values.len() {
            0 => None,
            1 => values.into_iter().next(),
            _ => Some(Value::Iterator(values))
        };

        Ok(context.identity())
    }

    fn traverse_term(&self, context: &mut TraversalContext, cache: &mut SharedCache, term: &Term) -> Result<Value, QueryError> {
        match term {
            Term::LookupByName(key) => {
                self.child(context, cache, key)?;
                Ok(context.identity())
            }
            Term::LookupKeyValueByName(key) => {
                self.child(context, cache, key)?;
                let value = context.identity();
                Ok(Value::KeyValue(Box::new(Value::Str(key.to_string())), Box::new(value)))
            }
            Term::LookupByIndex(i) => {
                self.index(context, *i);
                Ok(context.identity())
            }
            Term::ByIndexReverse(i) => {
                self.index_reverse(context, *i);
                Ok(context.identity())
            }
            Term::SliceData(from, to) => {
                self.slice(context, *from, *to)?;
                Ok(context.identity())
            }
            unexpected => {
                Err(QueryError::internal(format!("unhandled term in query: {:?}", unexpected)))
            }
        }
    }

    // Comma has be dealt with
    fn traverse_terms_inner(&self, context: &mut TraversalContext, cache: &mut SharedCache, terms: &[Term]) -> Result<Option<Value>, QueryError> {
        if terms.is_empty() {
            return Ok(None);
        }

        for term in terms {
            self.enter_foreign(context, cache)?;

            context.identity = match term {
                Term::NoOperation => {
                    context.identity.take()
                }
                Term::BoolLiteral(value) => Some(Value::Bool(*value)),
                Term::Select(lhs, op, rhs) => {
                    let was_stream = matches!(context.identity, Some(Value::Iterator(_)));
                    let elems = self.to_iterable(context, cache)?;

                    let result = iterate(elems, |v| {
                        let left = self.traverse(&mut context.clone_value(Some(v.clone())), cache, lhs)?;

                        let Some(op) = op else {
                            return Ok(match left {
                                Value::Bool(true) => Some(v),
                                _ => None
                            });
                        };
                        let right = self.traverse(&mut context.clone_value(Some(v.clone())), cache, rhs)?;

                        let selected = match op {
                            Compare::Equals => left == right,
                            Compare::NotEquals => left != right,
                            Compare::LessThan => left < right,
                            Compare::GreaterThan => left > right,
                            Compare::LessThanEq => left <= right,
                            Compare::GreaterThanEq => left >= right,
                        };
                        if selected {
                            Ok(Some(v))
                        } else {
                            Ok(None)
                        }
                    })?;
                    // select preserves the stream-ness of its input: filtering a
                    // stream yields a stream, filtering an array yields an array
                    match result {
                        Value::List(items) if was_stream => Some(Value::Iterator(items)),
                        other => Some(other),
                    }
                }
                Term::Contains(terms) => {
                    match self.traverse(&mut context.clone(), cache, terms)? {
                        Value::Str(substr) => {
                            let Some(value) = context.identity.take() else { return Ok(None); };
                            let Value::Str(field_string) = value else { return Ok(None); };
                            if field_string.contains(&substr) {
                                return Ok(Some(Value::Bool(true)));
                            }
                        }
                        wanted_contains => {
                            return Err(QueryError::type_error("contains", wanted_contains));
                        }
                    }
                    Some(Value::Bool(false))
                }
                Term::Iterator => {
                    Some(self.to_iterable(context, cache)?)
                }
                Term::Calculate(lhs, op, rhs) => {
                    let ident = context.identity.take();
                    let lhs_result = self.traverse(&mut context.clone_value(ident.clone()), cache, lhs)?;
                    let rhs_result = self.traverse(&mut context.clone_value(ident), cache, rhs)?;
                    let result = match op {
                        Operation::Addition => lhs_result.try_add(rhs_result)?,
                        Operation::Subtraction => lhs_result.try_sub(rhs_result)?,
                        Operation::Multiplication => {
                            return Err(QueryError::Unsupported("operator '*' is parsed but not implemented".to_string()));
                        }
                        Operation::Division => {
                            return Err(QueryError::Unsupported("operator '/' is parsed but not implemented".to_string()));
                        }
                    };
                    Some(result)
                }
                Term::SetVariable(name) => {
                    cache.variables
                        .insert(name.to_string(), self.identity(context));
                    context.identity.take()
                }
                Term::GetVariable(name) => {
                    Some(cache.variables.get(name).unwrap_or(&Value::Empty).clone())
                }
                Term::Reduce(outer_terms, init, terms) => {
                    // search for variables
                    let vars: Vec<&String> = outer_terms
                        .iter()
                        .filter_map(|term| match term {
                            Term::SetVariable(variable) => Some(variable),
                            _ => None,
                        })
                        .collect();
                    self.traverse_terms_inner(context, cache, outer_terms)?;

                    let initial = self.traverse(&mut context.clone_value(None), cache, init)?;
                    let Some(variable) = vars.first() else {
                        return Err(QueryError::internal("reduce expression without a variable binding"));
                    };

                    let value = cache
                        .variables
                        .get(variable.as_str())
                        .unwrap_or(&Value::Empty)
                        .clone();

                    let mut reduce_context = context.clone_value(Some(initial));

                    let result = reduce(value, |acc, v| {
                        cache.variables.insert(variable.to_string(), v);
                        reduce_context.identity = Some(acc);
                        self.traverse(&mut reduce_context, cache, terms)
                    })?;

                    Some(result)
                }
                Term::Map(terms) => {
                    let elems = self.to_iterable(context, cache)?;
                    let result = iterate(elems, |v| {
                        Ok(Some(self.traverse(&mut context.clone_value(Some(v)), cache, terms)?))
                    })?;
                    Some(result)
                }
                Term::ObjectConstruction(obj_terms) => {
                    if let Some(value) = context.identity.take() {
                        Some(iterate(value, |v| {
                            let output = self.traverse(&mut context.clone_value(Some(v)), cache, obj_terms)?;
                            Ok(Some(Value::Object(Box::new(output))))
                        })?)
                    } else {
                        let output = self.traverse(context, cache, obj_terms)?;
                        Some(Value::Object(Box::new(output)))
                    }
                }
                Term::KeyValue(key, value_terms) => {
                    let ident = context.identity.take();
                    let key = self.traverse(&mut context.clone_value(ident.clone()), cache, std::slice::from_ref(&**key))?;
                    let result = self.traverse(&mut context.clone_value(ident), cache, value_terms)?;
                    trace!("Term::kv result: {:?} {:?}", key, result);
                    match key {
                        Value::Empty | Value::List(_) | Value::Iterator(_) => None,
                        _ => {
                            Some(Value::KeyValue(Box::new(key), Box::new(result)))
                        }
                    }
                }
                Term::Identity => {
                    if context.current_file.is_none() && context.identity.is_none() {
                        let Some(store) = self.store else {
                            return Ok(Some(Value::Empty));
                        };
                        let mut exports: Vec<Value> = Vec::new();
                        for export in store.exports() {
                            let Some(spec) = store.spec_by_export(export) else { continue };
                            exports.push(Value::KeyValue(
                                Box::new(Value::Str(spec.file_name.to_string())),
                                Box::new(Value::List(vec![])),
                            ));
                        }
                        context.root_listing = true;
                        Some(Value::Object(Box::new(Value::List(exports))))
                    } else {
                        context.identity.take()
                    }
                }
                Term::ArrayConstruction(arr_terms) => {
                    let result = self.traverse(context, cache, arr_terms)?;
                    match result {
                        Value::Empty => Some(Value::List(Vec::with_capacity(0))),
                        Value::Iterator(values) => Some(Value::List(values)),
                        Value::List(_) => Some(result),
                        one_element => Some(Value::List(vec![one_element])),
                    }
                }
                Term::Length => match context.identity() {
                    Value::Str(string) => Some(Value::U64(string.chars().count() as u64)),
                    Value::List(list) => Some(Value::U64(list.len() as u64)),
                    Value::Iterator(iterable) => Some(Value::U64(iterable.len() as u64)),
                    Value::Row(file, _) => {
                        let fields = self.store
                            .and_then(|s| s.spec(&file))
                            .map(|spec| spec.file_fields.len())
                            .unwrap_or(0);
                        Some(Value::U64(fields as u64))
                    }
                    Value::Object(data) => {
                        match *data {
                            Value::List(pairs) | Value::Iterator(pairs) => Some(Value::U64(pairs.len() as u64)),
                            _ => Some(Value::U64(0))
                        }
                    }
                    Value::Empty => Some(Value::U64(0)),
                    value => return Err(QueryError::type_error("length", value))
                },
                Term::Keys => match context.identity() {
                    Value::Row(file, _) => {
                        let keys = self.store
                            .and_then(|s| s.spec(&file))
                            .map(|spec| {
                                spec.file_fields.iter()
                                    .map(|field| Value::Str(field.field_name.clone()))
                                    .collect()
                            })
                            .unwrap_or_default();
                        Some(Value::List(keys))
                    }
                    Value::Object(data) => {
                        match *data {
                            Value::List(pairs) | Value::Iterator(pairs) => {
                                let keys = pairs.iter().filter_map(|kv| match kv {
                                    Value::KeyValue(key, _) => {
                                        match *key.clone() {
                                            Value::Str(key) => Some(Value::Str(key)),
                                            _ => None,
                                        }
                                    }
                                    _ => None,
                                }).collect();
                                Some(Value::List(keys))
                            }
                            _ => None
                        }
                    }
                    value => return Err(QueryError::type_error("keys", value))
                },
                Term::Key(terms) => {
                    Some(self.traverse(context, cache, terms)?)
                }
                Term::StringLiteral(text) => {
                    Some(Value::Str(text.to_string()))
                }
                Term::Transpose => match context.identity() {
                    Value::List(values) => {
                        trace!("transpose input {:?}", values);

                        let mut lists = Vec::new();
                        for value in values {
                            if let Value::List(v) = value { lists.push(v) }
                        }

                        let max = lists
                            .iter()
                            .fold(0u64, |max, list| u64::max(max, list.len() as u64));

                        let mut outer = Vec::new();
                        for i in 0..max {
                            let inner = lists
                                .iter()
                                .map(|list| {
                                    list.get(i as usize).unwrap_or(&Value::Empty).clone()
                                })
                                .collect();

                            outer.push(Value::List(inner));
                        }
                        trace!("transpose output {:?}", outer);
                        Some(Value::List(outer))
                    }
                    unexpected => {
                        return Err(QueryError::type_error("transpose", unexpected));
                    }
                },
                Term::UnsignedNumber(value) => {
                    Some(Value::U64(*value))
                }
                Term::SignedNumber(value) => {
                    Some(Value::I64(*value))
                }
                _ => Some(self.traverse_term(context, cache, term)?)
            };
        }

        Ok(context.identity.take())
    }

    fn child(&self, context: &mut TraversalContext, cache: &mut SharedCache, name: &str) -> Result<(), QueryError> {
        trace!("entered {}", name);

        let spec: Option<&FileSpec> = self.store.and_then(|s| s.spec_by_export(name))
            .or_else(|| self.store.and_then(|s| s.spec_by_export(context.current_file.as_deref().unwrap_or(""))));

        let from_root = context.root_listing;
        context.root_listing = false;
        self.enter_foreign(context, cache)?;
        if let (Some(spec), None) = (spec, &context.current_file) {
            // the file is only loaded for its row count; fields are read lazily
            let rows_count = self.cached_file(cache, &spec.file_name)?.rows_count;

            let file_name: Rc<str> = Rc::from(spec.file_name.as_str());
            let values: Vec<Value> = (0..rows_count as u64)
                .map(|i| Value::Row(file_name.clone(), i))
                .collect();

            context.current_field = None;
            context.current_file = Some(spec.file_name.to_string());
            context.identity = Some(Value::List(values));
        } else {
            // a bare name at the root can only be a table; anything else is a
            // hard error so a misspelling doesn't silently turn into null
            if context.current_file.is_none() && (context.identity.is_none() || from_root) {
                if let Some(store) = self.store {
                    let tables = store.exports();
                    let suggestion = closest_name(name, tables.iter().copied());
                    return Err(QueryError::UnknownTable { name: name.to_string(), suggestion });
                }
            }
            context.current_field = Some(name.to_string());
            context.identity = Some(self.value(context, cache)?);
        }
        Ok(())
    }

    fn index(&self, context: &mut TraversalContext, index: usize) {
        let value = context.identity();
        context.identity = match value {
            Value::List(list) => list.into_iter().nth(index),
            Value::Str(str) => str.chars().nth(index).map(|value| Value::Str(value.to_string())),
            _ => None,
        };
    }

    fn index_reverse(&self, context: &mut TraversalContext, index: usize) {
        let value = context.identity();
        context.identity = match value {
            Value::List(list) => {
                list.len().checked_sub(index)
                    .and_then(|index| list.into_iter().nth(index))
            }
            Value::Str(str) => {
                str.chars().count().checked_sub(index)
                    .and_then(|index| str.chars().nth(index))
                    .map(|value| Value::Str(value.to_string()))
            }
            _ => None,
        };
    }

    fn slice(&self, context: &mut TraversalContext, from: i64, to: i64) -> Result<(), QueryError> {
        let value = context.identity();
        context.identity = match value {
            Value::List(list) => {
                let size = list.len();
                let from = if from.is_negative() { size.saturating_sub(from.unsigned_abs() as usize) } else { from as usize };
                let to = if to.is_negative() { size.saturating_sub(to.unsigned_abs() as usize) } else { to as usize };
                if from > to {
                    Some(Value::List(vec![]))
                } else {
                    let sliced = list[from..usize::min(to, list.len())].to_vec();
                    Some(Value::List(sliced))
                }
            }
            Value::Str(str) => {
                let size = str.len();
                let from = if from.is_negative() { size.saturating_sub(from.unsigned_abs() as usize) } else { from as usize };
                let to = if to.is_negative() { size.saturating_sub(to.unsigned_abs() as usize) } else { to as usize };
                if from > to {
                    Some(Value::List(vec![]))
                } else {
                    let to = min(to, str.len());
                    Some(Value::Str(str[from..to].to_string()))
                }
            }
            unexpected => {
                return Err(QueryError::type_error("slice", unexpected));
            }
        };
        Ok(())
    }

    fn to_iterable(&self, context: &mut TraversalContext, cache: &mut SharedCache) -> Result<Value, QueryError> {
        self.enter_foreign(context, cache)?;

        let value = context.identity();
        match value {
            Value::List(list) => Ok(Value::Iterator(list)),
            Value::Iterator(list) => Ok(Value::Iterator(list)),
            Value::Row(file_name, row) => {
                match self.materialize_row(cache, &file_name, row)? {
                    Value::Object(content) => match *content {
                        Value::List(fields) | Value::Iterator(fields) => Ok(Value::Iterator(fields)),
                        _ => Ok(Value::Iterator(Vec::with_capacity(0))),
                    },
                    _ => Ok(Value::Iterator(Vec::with_capacity(0))),
                }
            }
            Value::Object(content) => {
                let fields = match *content {
                    Value::List(fields) | Value::Iterator(fields) => fields,
                    unexpected => {
                        return Err(QueryError::type_error("iteration", unexpected));
                    }
                };
                Ok(Value::Iterator(fields))
            }
            Value::Empty => Ok(Value::Iterator(Vec::with_capacity(0))),
            unexpected => Err(QueryError::type_error("iteration", unexpected)),
        }
    }

    fn value(&self, context: &mut TraversalContext, cache: &mut SharedCache) -> Result<Value, QueryError> {
        if context.identity.is_none() {
            return Ok(Value::Empty);
        }
        let wanted = context.current_field.clone();

        match context.identity.take().unwrap() {
            Value::Object(entries) => {
                let wanted = wanted.as_deref()
                    .ok_or_else(|| QueryError::internal("field lookup without a field name"))?;
                match *entries {
                    Value::List(list) | Value::Iterator(list) => {
                        let mut values = Vec::new();
                        for field in list {
                            if let Value::KeyValue(key, value) = field {
                                if matches!(key.as_ref(), Value::Str(k) if k.as_str() == wanted) {
                                    values.push(*value);
                                }
                            }
                        }

                        Ok(values.into_iter().next().unwrap_or(Value::Empty))
                    }
                    Value::KeyValue(key, value) => {
                        if matches!(key.as_ref(), Value::Str(k) if k.as_str() == wanted) {
                            Ok(*value)
                        } else {
                            Ok(Value::Empty)
                        }
                    }
                    unexpected => {
                        Err(QueryError::internal(format!("failed to extract Value::Object, object contained {}", unexpected)))
                    }
                }
            }
            Value::Row(file_name, row) => {
                let wanted = wanted.as_deref()
                    .ok_or_else(|| QueryError::internal("field lookup without a field name"))?;
                context.current_file = Some(file_name.to_string());
                self.read_row_field(cache, &file_name, row, wanted)
            }
            Value::Iterator(values) => {
                let wanted = wanted.as_deref()
                    .ok_or_else(|| QueryError::internal("field lookup without a field name"))?;
                let mut row_file: Option<Rc<str>> = None;
                let mut result = Vec::new();
                for value in values {
                    let item = match value {
                        Value::Row(file_name, row) => {
                            let item = self.read_row_field(cache, &file_name, row, wanted)?;
                            row_file = Some(file_name);
                            item
                        }
                        Value::KeyValue(k, v) => {
                            if matches!(k.as_ref(), Value::Str(k) if k.as_str() == wanted) {
                                *v
                            } else {
                                Value::Empty
                            }
                        }
                        Value::Object(elements) => {
                            let obj = match *elements {
                                Value::List(fields) | Value::Iterator(fields) => fields,
                                unexpected => {
                                    return Err(QueryError::internal(format!("type {} unexpected in Value::Object", unexpected)));
                                }
                            };

                            let mut first = Value::Empty;
                            for kv in obj {
                                match kv {
                                    Value::KeyValue(k, v) => {
                                        if matches!(k.as_ref(), Value::Str(k) if k.as_str() == wanted) {
                                            first = *v;
                                            break;
                                        }
                                    }
                                    unexpected => {
                                        return Err(QueryError::internal(format!("failed to extract Value::Object, object contained {}", unexpected)));
                                    }
                                }
                            }
                            first
                        }
                        unexpected => {
                            return Err(QueryError::type_error("iteration", unexpected));
                        }
                    };
                    result.push(item);
                }

                if let Some(file_name) = row_file {
                    context.current_file = Some(file_name.to_string());
                }
                Ok(Value::List(result))
            }
            Value::U64(i) => {
                let current = context.current_file.clone()
                    .ok_or_else(|| QueryError::internal("row lookup without a current table"))?;
                let store = self.store
                    .ok_or_else(|| QueryError::internal("no data store loaded"))?;
                let spec = store.spec(&current)
                    .ok_or_else(|| QueryError::internal(format!("no specification for table '{}'", current)))?;
                let file = self.cached_file(cache, &current)?;

                // TODO: extract to function
                let mut kv_list = Vec::with_capacity(spec.file_fields.len());
                for field in &spec.file_fields {
                    kv_list.push(Value::KeyValue(
                        Box::new(Value::Str(field.field_name.clone())),
                        Box::new(file.read_field(i, field)?),
                    ));
                }

                Ok(Value::Object(Box::new(Value::List(kv_list))))
            }
            _ => Ok(Value::Empty),
        }
    }

    fn identity(&self, context: &mut TraversalContext) -> Value {
        context.identity.clone().unwrap_or(Value::Empty)
    }

    fn enter_foreign(&self, context: &mut TraversalContext, cache: &mut SharedCache) -> Result<(), QueryError> {
        let current_spec: Option<&FileSpec> = context
            .current_file.as_ref()
            .and_then(|file| self.store.and_then(|s| s.spec(file)));
        let current_field = current_spec
            .and_then(|spec| {
                spec.file_fields.iter().find(|&field| {
                    context.current_field.as_deref() == Some(field.field_name.as_str())
                })
            });

        if let Some(current_field) = current_field.filter(|x| x.is_foreign_key()) {
            trace!("enter_foreign on field {:?}", current_field);
            context.current_field = None;

            let fk_name = current_field.file_name.as_ref()
                .ok_or_else(|| QueryError::internal("foreign key field without a target table"))?;
            let foreign_spec = self.store.and_then(|s| s.spec(fk_name))
                .ok_or_else(|| QueryError::internal(format!("foreign key target '{}' has no specification", fk_name)))?;

            let value = context.identity();
            let value = match value {
                Value::List(items) => Value::Iterator(items),
                _ => value,
            };

            let result = iterate(value, |v| {
                // already a resolved row handle, nothing to do
                if matches!(v, Value::Row(_, _)) {
                    return Ok(Some(v));
                }
                let raw_ids = match v {
                    Value::List(ids) => ids,
                    Value::Iterator(ids) => ids,
                    Value::U64(id) => vec![Value::U64(id)],
                    Value::Empty => vec![],
                    unexpected => {
                        return Err(QueryError::type_error("foreign key lookup", unexpected));
                    }
                };
                let mut ids = Vec::with_capacity(raw_ids.len());
                for id in raw_ids {
                    match id {
                        Value::U64(i) => ids.push(i),
                        Value::List(_) => {}
                        unexpected => {
                            return Err(QueryError::type_error("foreign key lookup", unexpected));
                        }
                    }
                }

                let rows = self.rows_from(fk_name, ids.as_slice())?;
                Ok(Some(rows))
            })?;

            context.current_field = None;
            context.current_file = Some(foreign_spec.file_name.clone());
            context.identity = Some(result);
        }
        Ok(())
    }

    fn rows_from(&self, filepath: &str, indices: &[u64]) -> Result<Value, QueryError> {
        let foreign_spec = self.store.and_then(|s| s.spec(filepath))
            .ok_or_else(|| QueryError::internal(format!("no specification for table '{}'", filepath)))?;

        let file_name: Rc<str> = Rc::from(foreign_spec.file_name.as_str());
        let values: Vec<Value> = indices
            .iter()
            .map(|i| Value::Row(file_name.clone(), *i))
            .collect();

        if values.len() > 1 {
            Ok(Value::List(values))
        } else {
            Ok(values.into_iter().next().unwrap_or(Value::Empty))
        }
    }
}

impl TraversalContext {
    pub fn clone_value(&self, ident: Option<Value>) -> Self {
        Self {
            current_field: self.current_field.clone(),
            current_file: self.current_file.clone(),
            identity: ident,
            root_listing: false,
        }
    }

    pub fn identity(&mut self) -> Value {
        self.identity.take().unwrap_or(Value::Empty)
    }
}
