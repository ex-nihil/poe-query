use std::cmp::min;
use std::collections::HashMap;
use std::process;

use log::*;

use crate::{Term};
use crate::dat::file::DatFile;
use crate::dat::DatStoreImpl;
use crate::dat::specification::{FieldSpecImpl, FileSpec};
use crate::query::{Compare, Operation};
use crate::traversal::{StaticContext, QueryProcessor};
use crate::traversal::utils::{iterate, reduce};

use super::value::Value;

/** entry point */
impl QueryProcessor for StaticContext<'_> {
    fn process(&self, terms: &[Term]) -> Value {
        self.traverse(&mut TraversalContext::default(), &mut SharedCache::default(), terms)
    }
}

/** Shared mutable data during traversal */
#[derive(Default)]
pub struct SharedCache {
    variables: HashMap<String, Value>,
    files: HashMap<String, DatFile>,
}

/** Local mutable data during traversal */
#[derive(Debug, Clone, Default)]
struct TraversalContext {
    current_field: Option<String>,
    current_file: Option<String>,
    identity: Option<Value>,
}

trait DataTraverser<'a> {
    fn traverse(&self, context: &mut TraversalContext, cache: &mut SharedCache, parsed_terms: &[Term]) -> Value;
    fn traverse_term(&self, context: &mut TraversalContext, cache: &mut SharedCache, term: &Term) -> Value;
    fn traverse_terms_inner(&self, context: &mut TraversalContext, cache: &mut SharedCache, terms: &[Term]) -> Option<Value>;

    fn child(&self, context: &mut TraversalContext, cache: &mut SharedCache, name: &str);
    fn index(&self, context: &mut TraversalContext, index: usize);
    fn index_reverse(&self, context: &mut TraversalContext, index: usize);
    fn slice(&self, context: &mut TraversalContext, from: i64, to: i64);
    fn to_iterable(&self, context: &mut TraversalContext, cache: &mut SharedCache) -> Value;
    fn value(&self, context: &mut TraversalContext) -> Value;
    fn identity(&self, context: &mut TraversalContext) -> Value;

    fn enter_foreign(&self, context: &mut TraversalContext, cache: &mut SharedCache);
    fn rows_from(&self, cache: &mut SharedCache, file: &str, indices: &[u64]) -> Value;
}

impl<'a> DataTraverser<'a> for StaticContext<'a> {
    fn traverse(&self, context: &mut TraversalContext, cache: &mut SharedCache, parsed_terms: &[Term]) -> Value {
        let values: Vec<Value> = if parsed_terms.contains(&Term::PipeOperator) {
            let mut ident = context.identity();
            for terms in parsed_terms.split(|term| matches!(term, Term::PipeOperator)) {
                let mut c = context.clone_value(Some(ident));
                ident = self.traverse(&mut c, cache, terms);
                context.current_file = c.current_file;
                context.current_field = c.current_field;
            }
            vec![ident]
        } else if parsed_terms.contains(&Term::CommaSeparator) {
            parsed_terms
                .split(|term| matches!(term, Term::CommaSeparator))
                .map(|terms| self.traverse(&mut context.clone(), cache, terms))
                .collect()
        } else {
            vec![self
                .traverse_terms_inner(context, cache, parsed_terms)
                .unwrap_or(Value::Empty)]
        };

        context.identity = match values.len() {
            0 => None,
            1 => values.into_iter().next(),
            _ => Some(Value::Iterator(values))
        };

        context.identity()
    }

    fn traverse_term(&self, context: &mut TraversalContext, cache: &mut SharedCache, term: &Term) -> Value {
        match term {
            Term::LookupByName(key) => {
                self.child(context, cache, key);
                context.identity()
            }
            Term::LookupKeyValueByName(key) => {
                self.child(context, cache, key);
                let asd = context.identity();
                Value::KeyValue(Box::new(Value::Str(key.to_string())), Box::new(asd))
            }
            Term::LookupByIndex(i) => {
                self.index(context, *i);
                context.identity()
            }
            Term::ByIndexReverse(i) => {
                self.index_reverse(context, *i);
                context.identity()
            }
            Term::SliceData(from, to) => {
                self.slice(context, *from, *to);
                context.identity()
            }
            unexpected => {
                error!("Unhandled term in query: {:?}.", unexpected);
                process::exit(-1);
            }
        }
    }

    // Comma has be dealt with
    fn traverse_terms_inner(&self, context: &mut TraversalContext, cache: &mut SharedCache, terms: &[Term]) -> Option<Value> {
        if terms.is_empty() {
            return None;
        }

        for term in terms {
            self.enter_foreign(context, cache);

            context.identity = match term {
                Term::NoOperation => {
                    context.identity.take()
                }
                Term::BoolLiteral(value) => Some(Value::Bool(*value)),
                Term::Select(lhs, op, rhs) => {
                    let elems = self.to_iterable(context, cache);

                    let result = iterate(elems, |v| {
                        let left = self.traverse(&mut context.clone_value(Some(v.clone())), cache, lhs);
                        let right = self.traverse(&mut context.clone_value(Some(v.clone())), cache, rhs);

                        let Some(op) = op else {
                            return match left {
                                Value::Bool(true) => Some(v),
                                _ => None
                            };
                        };

                        let selected = match op {
                            Compare::Equals => left == right,
                            Compare::NotEquals => left != right,
                            Compare::LessThan => left < right,
                            Compare::GreaterThan => left > right,
                            Compare::LessThanEq => left <= right,
                            Compare::GreaterThanEq => left >= right,
                        };
                        if selected {
                            Some(v)
                        } else {
                            None
                        }
                    });
                    Some(result)
                }
                Term::Contains(terms) => {
                    match self.traverse(&mut context.clone(), cache, terms) {
                        Value::Str(substr) => {
                            let Some(value) = context.identity.take() else { return None; };
                            let Value::Str(field_string) = value else { return None; };
                            if field_string.contains(&substr) {
                                return Some(Value::Bool(true));
                            }
                        }
                        wanted_contains => {
                            error!("Unsupported contains type: {:?}", wanted_contains);
                            process::exit(-1);
                        }
                    }
                    Some(Value::Bool(false))
                }
                Term::Iterator => {
                    Some(self.to_iterable(context, cache))
                }
                Term::Calculate(lhs, op, rhs) => {
                    let lhs_result = self.traverse(&mut context.clone(), cache, lhs);
                    let rhs_result = self.traverse(&mut context.clone(), cache, rhs);
                    let result = match op {
                        Operation::Addition => lhs_result + rhs_result,
                        Operation::Subtraction => lhs_result - rhs_result,
                        _ => Value::Empty,
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
                    self.traverse_terms_inner(context, cache, outer_terms);

                    let initial = self.traverse(&mut context.clone_value(None), cache, init);
                    let Some(variable) = vars.first() else {
                        return None;
                    };

                    let value = cache
                        .variables
                        .get(variable.as_str())
                        .unwrap_or(&Value::Empty)
                        .clone();

                    let mut reduce_context = context.clone_value(Some(initial));

                    let result = reduce(value, &mut |acc, v| {
                        cache.variables.insert(variable.to_string(), v);
                        reduce_context.identity = Some(acc);
                        self.traverse(&mut reduce_context, cache, terms)
                    });

                    Some(result)
                }
                Term::Map(terms) => {
                    let result = iterate(self.to_iterable(context, cache), |v| {
                        Some(self.traverse(&mut context.clone_value(Some(v)), cache, terms))
                    });
                    Some(result)
                }
                Term::ObjectConstruction(obj_terms) => {
                    if let Some(value) = context.identity.take() {
                        Some(iterate(value, |v| {
                            let output = self.traverse(&mut context.clone_value(Some(v)), cache, obj_terms);
                            Some(Value::Object(Box::new(output)))
                        }))
                    } else {
                        let output = self.traverse(context, cache, obj_terms);
                        Some(Value::Object(Box::new(output)))
                    }
                }
                Term::KeyValue(key, value_terms) => {
                    let key = self.traverse(&mut context.clone(), cache, &[*key.clone()]);
                    let result = self.traverse(&mut context.clone(), cache, &value_terms.to_vec());
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
                        if self.store.is_none() {
                            return Some(Value::Empty);
                        }
                        let exports: Vec<Value> = self
                            .store.unwrap()
                            .exports()
                            .iter()
                            .map(|export| {
                                let spec = self.store.unwrap().spec_by_export(export).unwrap();

                                Value::KeyValue(
                                    Box::new(Value::Str(spec.file_name.to_string())),
                                    Box::new(Value::List(vec![])),
                                )
                            })
                            .collect();
                        Some(Value::Object(Box::new(Value::List(exports))))
                    } else {
                        context.identity.take()
                    }
                }
                Term::ArrayConstruction(arr_terms) => {
                    let result = self.traverse(context, cache, &arr_terms.to_vec());
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
                    Value::Object(data) => {
                        match *data {
                            Value::List(pairs) | Value::Iterator(pairs) => Some(Value::U64(pairs.len() as u64)),
                            _ => Some(Value::U64(0))
                        }
                    }
                    Value::Empty => Some(Value::U64(0)),
                    value => unimplemented!("Unsupported type '{:?}' for 'length' operation", value)
                },
                Term::Keys => match context.identity() {
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
                    value => unimplemented!("Unsupported type '{:?}' for 'keys' operation", value)
                },
                Term::Key(terms) => {
                    Some(self.traverse(context, cache, terms))
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
                        error!("Transpose is only supported on lists. Attempted on type: {}.", unexpected);
                        process::exit(-1);
                    }
                },
                Term::UnsignedNumber(value) => {
                    Some(Value::U64(*value))
                }
                Term::SignedNumber(value) => {
                    Some(Value::I64(*value))
                }
                _ => Some(self.traverse_term(context, cache, term))
            };
        }

        context.identity.take()
    }

    fn child(&self, context: &mut TraversalContext, cache: &mut SharedCache, name: &str) {
        trace!("entered {}", name);

        let spec: Option<&FileSpec> = self.store.and_then(|s| s.spec_by_export(name))
            .or_else(|| self.store.and_then(|s| s.spec_by_export(context.current_file.as_ref().unwrap_or(&"".to_string()))));

        self.enter_foreign(context, cache);
        if let (Some(spec), None) = (spec, &context.current_file) {
            // generate initial values
            let file = cache.files.entry(spec.file_name.to_string()).or_insert_with(|| self.store.unwrap().file_by_filename(&spec.file_name).unwrap());

            let values: Vec<Value> = (0..file.rows_count)
                .map(|i| {
                    let kv_list: Vec<Value> = spec
                        .file_fields
                        .iter()
                        .map(|field| {
                            Value::KeyValue(
                                Box::new(Value::Str(field.field_name.clone())),
                                Box::new(file.read_field(i as u64, field)),
                            )
                        })
                        .collect();
                    Value::Object(Box::new(Value::List(kv_list)))
                })
                .collect();

            context.current_field = None;
            context.current_file = Some(spec.file_name.to_string());
            context.identity = Some(Value::List(values));
        } else {
            context.current_field = Some(name.to_string());
            context.identity = Some(self.value(context));
        }
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
                let index = list.len() - index;
                list.into_iter().nth(index)
            }
            Value::Str(str) => {
                let index = str.chars().count() - index;
                str.chars().nth(index).map(|value| Value::Str(value.to_string()))
            }
            _ => None,
        };
    }

    fn slice(&self, context: &mut TraversalContext, from: i64, to: i64) {
        let value = context.identity();
        context.identity = match value {
            Value::List(list) => {
                let size = list.len();
                let from = if from.is_negative() { size - from.unsigned_abs() as usize } else { from as usize };
                let to = if to.is_negative() { size - to.unsigned_abs() as usize } else { to as usize };
                if from > to {
                    Some(Value::List(vec![]))
                } else {
                    let sliced = list[from..usize::min(to, list.len())].to_vec();
                    Some(Value::List(sliced))
                }
            }
            Value::Str(str) => {
                let size = str.len();
                let from = if from.is_negative() { size - from.unsigned_abs() as usize } else { from as usize };
                let to = if to.is_negative() { size - to.unsigned_abs() as usize } else { to as usize };
                if from > to {
                    Some(Value::List(vec![]))
                } else {
                    let to = min(to, str.len());
                    Some(Value::Str(str[from..to].to_string()))
                }
            }
            unexpected => {
                error!("Type {unexpected} cannot be sliced/indexed");
                process::exit(-1);
            }
        };
    }

    fn to_iterable(&self, context: &mut TraversalContext, cache: &mut SharedCache) -> Value {
        self.enter_foreign(context, cache);

        let value = context.identity();
        match value {
            Value::List(list) => Value::Iterator(list),
            Value::Iterator(list) => Value::Iterator(list),
            Value::Object(content) => {
                let fields = match *content {
                    Value::List(fields) | Value::Iterator(fields) => fields,
                    unexpected => {
                        error!("Type {unexpected} cannot be iterated over");
                        process::exit(-1);
                    }
                };
                Value::Iterator(fields)
            }
            Value::Empty => Value::Iterator(Vec::with_capacity(0)),
            unexpected => {
                error!("Type {unexpected} cannot be iterated over");
                process::exit(-1);
            }
        }
    }

    fn value(&self, context: &mut TraversalContext) -> Value {
        if context.identity.is_none() {
            return Value::Empty;
        }

        match context.identity.take().unwrap() {
            Value::Object(entries) => {
                match *entries {
                    Value::List(list) | Value::Iterator(list) => {
                        let mut values = Vec::new();
                        for field in list {
                            if let Value::KeyValue(key, value) = field {
                                if *key == Value::Str(context.current_field.clone().unwrap()) {
                                    values.push(*value);
                                }
                            }
                        }

                        values.into_iter().next().unwrap_or(Value::Empty)
                    }
                    Value::KeyValue(key, value) => {
                        if *key == Value::Str(context.current_field.clone().unwrap()) {
                            *value
                        } else {
                            Value::Empty
                        }
                    }
                    unexpected => {
                        error!("failed to extract Value::Object. Object contained {}", unexpected);
                        process::exit(-1);
                    }
                }
            }
            Value::Iterator(values) => {
                let mut result = Vec::new();
                for value in values {
                    let item = match value {
                        Value::KeyValue(k, v) => {
                            if Value::Str(context.current_field.clone().unwrap()) == *k {
                                *v
                            } else {
                                Value::Empty
                            }
                        }
                        Value::Object(elements) => {
                            let obj = match *elements {
                                Value::List(fields) | Value::Iterator(fields) => fields,
                                unexpected => {
                                    error!("Type {unexpected} unexpected in Value::Object");
                                    process::exit(-1);
                                }
                            };

                            let mut first = Value::Empty;
                            for kv in obj {
                                match kv {
                                    Value::KeyValue(k, v) => {
                                        if Value::Str(context.current_field.clone().unwrap()) == *k {
                                            first = *v;
                                            break;
                                        }
                                    }
                                    unexpected => {
                                        error!("failed to extract Value::Object. Object contained {}", unexpected);
                                        process::exit(-1);
                                    }
                                }
                            }
                            first
                        }
                        unexpected => {
                            error!("Unable to to iterate over {}.", unexpected);
                            process::exit(-1);
                        }
                    };
                    result.push(item);
                }

                Value::List(result)
            }
            Value::U64(i) => {
                let current = context.current_file.as_ref().unwrap();
                let spec = self.store.unwrap().spec(current).unwrap();
                let file = self.store.unwrap().file_by_filename(current).unwrap();

                // TODO: extract to function
                let kv_list: Vec<Value> = spec
                    .file_fields
                    .iter()
                    .map(move |field| {
                        Value::KeyValue(
                            Box::new(Value::Str(field.field_name.clone())),
                            Box::new(file.read_field(i, field)),
                        )
                    })
                    .collect();

                Value::Object(Box::new(Value::List(kv_list)))
            }
            _ => Value::Empty,
        }
    }

    fn identity(&self, context: &mut TraversalContext) -> Value {
        context.identity.clone().unwrap_or(Value::Empty)
    }

    fn enter_foreign(&self, context: &mut TraversalContext, cache: &mut SharedCache) {
        let current_spec: Option<&FileSpec> = context
            .current_file.as_ref()
            .and_then(|file| self.store.unwrap().spec(file));
        let current_field = current_spec
            .and_then(|spec| {
                spec.file_fields.iter().find(|&field| {
                    context.current_field.is_some()
                        && context.current_field.clone().unwrap() == field.field_name
                })
            });

        if let Some(current_field) = current_field.filter(|x| x.is_foreign_key()) {
            trace!("enter_foreign on field {:?}", current_field);
            context.current_field = None;

            let fk_name = &current_field.file_name.as_ref().unwrap();
            let foreign_spec = self.store.unwrap().spec(fk_name).unwrap();

            let value = context.identity();
            let value = match value {
                Value::List(items) => Value::Iterator(items),
                _ => value,
            };

            let result = iterate(value, |v| {
                let ids: Vec<u64> = match v {
                    Value::List(ids) => ids,
                    Value::Iterator(ids) => ids,
                    Value::U64(id) => vec![Value::U64(id)],
                    Value::Empty => vec![],
                    unexpected => {
                        error!("Not a valid id for foreign key {}.", unexpected);
                        process::exit(-1);
                    }
                }
                    .iter()
                    .filter_map(|v| match v {
                        Value::U64(i) => Some(*i),
                        Value::List(_) => None,
                        unexpected => {
                            error!("Unexpected value {} in enter_foreign.", unexpected);
                            process::exit(-1);
                        }
                    })
                    .collect();

                let rows = self.rows_from(cache, current_field.file_name.as_ref().unwrap(), ids.as_slice());
                Some(rows)
            });

            context.current_field = None;
            context.current_file = Some(foreign_spec.file_name.clone());
            context.identity = Some(result);
        }
    }

    fn rows_from(&self, cache: &mut SharedCache, filepath: &str, indices: &[u64]) -> Value {
        let foreign_spec = self.store.unwrap().spec(filepath).unwrap();

        let file = cache.files.entry(filepath.to_string()).or_insert_with(|| self.store.unwrap().file_by_filename(filepath).unwrap());

        let values: Vec<Value> = indices
            .iter()
            .map(|i| {
                let kv_list: Vec<Value> = foreign_spec
                    .file_fields
                    .iter()
                    .map(|field| {
                        Value::KeyValue(
                            Box::new(Value::Str(field.field_name.clone())),
                            Box::new(file.read_field(*i, field)),
                        )
                    })
                    .collect();
                Value::Object(Box::new(Value::List(kv_list)))
            })
            .collect();

        if values.len() > 1 {
            Value::List(values)
        } else {
            values.into_iter().next().unwrap_or(Value::Empty)
        }
    }
}

impl TraversalContext {
    pub fn clone_value(&self, ident: Option<Value>) -> Self {
        Self {
            current_field: self.current_field.clone(),
            current_file: self.current_file.clone(),
            identity: ident,
        }
    }

    pub fn identity(&mut self) -> Value {
        self.identity.take().unwrap_or(Value::Empty)
    }
}
