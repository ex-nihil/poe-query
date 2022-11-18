use std::collections::HashMap;

use log::*;

use crate::{DatReader, Term};
use crate::dat::file::DatFile;
use crate::dat::DatStoreImpl;
use crate::dat::specification::{FieldSpecImpl, FileSpec};
use crate::query::{Compare, Operation};

use super::value::Value;

pub struct StaticContext<'a> {
    pub store: Option<&'a DatReader<'a>>,
}

/** Local context */
#[derive(Debug, Clone)]
pub struct TraversalContext<'a> {
    pub current_field: Option<String>,
    pub current_file: Option<String>,
    pub dat_file: Option<&'a DatFile>,
    pub identity: Option<Value>,
}

impl Default for TraversalContext<'_> {
    fn default() -> Self {
        Self {
            current_field: None,
            current_file: None,
            dat_file: None,
            identity: None,
        }
    }
}

impl Default for StaticContext<'_> {
    fn default() -> Self {
        Self {
            store: None
        }
    }
}

/** Shared cache */
pub struct SharedCache {
    pub variables: HashMap<String, Value>,
    pub files: HashMap<String, DatFile>,
}

impl TraversalContext<'_> {
    pub fn clone_value(&self, ident: Option<Value>) -> Self {
        Self {
            current_field: self.current_field.clone(),
            current_file: self.current_file.clone(),
            dat_file: self.dat_file,
            identity: ident
        }
    }
}

impl Default for SharedCache {
    fn default() -> Self {
        Self {
            variables: Default::default(),
            files: Default::default()
        }
    }
}

pub trait TraversalContextImpl<'a> {
    fn child(&self, context: &mut TraversalContext, cache: &mut SharedCache, name: &str);
    fn index(&self, context: &mut TraversalContext, index: usize);
    fn index_reverse(&self, context: &mut TraversalContext, index: usize);
    fn slice(&self, context: &mut TraversalContext, from: usize, to: usize);
    fn to_iterable(&self, context: &mut TraversalContext, cache: &mut SharedCache) -> Value;
    fn value(&self, context: &mut TraversalContext) -> Value;
    fn identity(&self, context: &mut TraversalContext) -> Value;

    fn enter_foreign(&self, context: &mut TraversalContext, cache: &mut SharedCache);
    fn rows_from(&self, cache: &mut SharedCache, file: &str, indices: &[u64]) -> Value;
}

pub trait TermsProcessor {
    fn process_terms(&self, terms: &[Term]) -> Value;
    fn process(&self, context: &mut TraversalContext, cache: &mut SharedCache, parsed_terms: &[Term]) -> Value;
    fn traverse_term(&self, context: &mut TraversalContext, cache: &mut SharedCache, term: &Term) -> Value;
    fn traverse_terms_inner(&self, context: &mut TraversalContext, cache: &mut SharedCache, terms: &[Term]) -> Option<Value>;
}

impl TermsProcessor for StaticContext<'_> {
    fn process_terms(&self, terms: &[Term]) -> Value {
        self.process(&mut TraversalContext::default(), &mut SharedCache::default(), terms)
    }

    fn process(&self, context: &mut TraversalContext, cache: &mut SharedCache, parsed_terms: &[Term]) -> Value {
        let values: Vec<Value> = if parsed_terms.contains(&Term::comma) {
            parsed_terms
                .split(|term| match term {
                    Term::comma => true,
                    _ => false,
                })
                .filter_map(|terms| self.traverse_terms_inner(&mut context.clone(), cache, &terms))
                .collect()
        } else {
            vec![self
                .traverse_terms_inner(context, cache, parsed_terms)
                .unwrap_or(Value::Empty)]
        };



        context.identity = if values.len() > 1 {
            Some(Value::List(values))
        } else if values.len() == 1 {
            values.into_iter().nth(0)
        } else {
            None
        };

        context.identity.take().unwrap_or(Value::Empty)
    }

    fn traverse_term(&self, context: &mut TraversalContext, cache: &mut SharedCache, term: &Term) -> Value {
        match term {
            Term::by_name(key) => {
                self.child(context, cache, key);
                let asd = context.identity.take().unwrap_or(Value::Empty);
                asd
            }
            Term::by_index(i) => {
                self.index(context, *i);
                context.identity.take().unwrap_or(Value::Empty)
            }
            Term::by_index_reverse(i) => {
                self.index_reverse(context, *i);
                context.identity.take().unwrap_or(Value::Empty)
            }
            Term::slice(from, to) => {
                self.slice(context, *from, *to);
                context.identity.take().unwrap_or(Value::Empty)
            }
            _ => panic!("unhandled term: {:?}", term),
        }
    }

    // Comma has be dealt with
    fn traverse_terms_inner(&self, context: &mut TraversalContext, cache: &mut SharedCache, terms: &[Term]) -> Option<Value> {
        if terms.is_empty() {
            return None;
        }

        for term in terms {
            context.identity = match term {
                Term::select(lhs, op, rhs) => {
                    let elems = self.to_iterable(context, cache);
                    let result = iterate(elems, |v| {

                        let left = self.process(&mut context.clone_value(Some(v.clone())), cache, lhs);
                        let right = self.process(&mut context.clone_value(Some(v.clone())), cache, rhs);

                        let selected = match op {
                            Compare::equals => left == right,
                            Compare::not_equals => left != right,
                            Compare::less_than => left < right,
                            Compare::greater_than => left > right,
                            Compare::less_than_eq =>  left <= right,
                            Compare::greater_than_eq => left >= right,
                        };
                        if selected {
                            Some(v)
                        } else {
                            None
                        }
                    });
                    Some(result)
                },
                Term::noop => {
                    context.identity.take()
                },
                Term::iterator => {
                    Some(self.to_iterable(context, cache))
                },
                Term::calculate(lhs, op, rhs) => {
                    let lhs_result = self.traverse_terms_inner(&mut context.clone(), cache, lhs);
                    let rhs_result = self.traverse_terms_inner(&mut context.clone(), cache, rhs);
                    let result = match op {
                        // TODO: add operation support on the different types
                        Operation::add => lhs_result.unwrap() + rhs_result.unwrap(),
                        _ => Value::Empty,
                    };
                    Some(result)
                },
                Term::set_variable(name) => {
                    cache.variables
                        .insert(name.to_string(), self.identity(context));
                    context.identity.take()
                },
                Term::get_variable(name) => {
                    Some(cache.variables.get(name).unwrap_or(&Value::Empty).clone())
                },
                Term::reduce(outer_terms, init, terms) => {
                    // seach for variables
                    let vars: Vec<&String> = outer_terms
                        .iter()
                        .filter_map(|term| match term {
                            Term::set_variable(variable) => Some(variable),
                            _ => None,
                        })
                        .collect();
                    self.traverse_terms_inner(context, cache, outer_terms);


                    let initial = self.traverse_terms_inner(&mut context.clone_value(None), cache, init);
                    let variable = vars.first().unwrap().as_str();
                    let value = cache
                        .variables
                        .get(variable)
                        .unwrap_or(&Value::Empty)
                        .clone();

                    //context.identity = initial;

                    let mut reduce_context = context.clone_value(initial);
                    /*
                    let result = reduce(&value, &mut |v| {
                        cache.variables.insert(variable.to_string(), v.clone());
                        context.identity = Some(self.process(context, cache, terms));
                        warn!("Term::reduce cloned {:?}", context.identity);
                        context.identity.clone()
                    });
                     */
                    let result = reduce(value, &mut |acc, v| {
                        cache.variables.insert(variable.to_string(), v.clone());
                        reduce_context.identity = Some(acc);
                        self.process(&mut reduce_context, cache, terms)
                    });

                    Some(result)
                },
                Term::map(terms) => {
                    let result = iterate(self.to_iterable(context, cache), |v| {
                        Some(self.process(&mut context.clone_value(Some(v)), cache, terms))
                    });
                    Some(result)
                },
                Term::object(obj_terms) => {
                    if let Some(value) = context.identity.take() {
                        Some(iterate(value, |v| {
                            let output = self.process(&mut context.clone_value(Some(v)), cache, obj_terms);
                            Some(Value::Object(Box::new(output)))
                        }))
                    } else {
                        let output = self.process(&mut context.clone(), cache, obj_terms);
                        Some(Value::Object(Box::new(output)))
                    }
                },
                Term::kv(key, value_terms) => {
                    let key = self.process(&mut context.clone(), cache, &vec![*key.clone()]);
                    let result = self.process(&mut context.clone(), cache, &value_terms.to_vec());
                    trace!("Term::kv result: {:?} {:?}", key, result);
                    match key {
                        Value::Empty | Value::List(_) => None,
                        _ => {
                            Some(Value::KeyValue(Box::new(key), Box::new(result)))
                        }
                    }
                },
                Term::identity => {
                    if context.current_file.is_none() && context.identity.is_none() {
                        let exports: Vec<Value> = self
                            .store.unwrap()
                            .exports()
                            .iter()
                            .map(|export| {
                                let spec = self.store.unwrap().spec_by_export(export).unwrap();

                                Value::KeyValue(
                                    Box::new(Value::Str(spec.filename.to_string())),
                                    Box::new(Value::List(vec![])),
                                )
                            })
                            .collect();
                        Some(Value::Object(Box::new(Value::List(exports))))
                    } else {
                        context.identity.take()
                    }
                },
                Term::array(arr_terms) => {
                    let result = self.process(&mut context.clone(), cache, &arr_terms.to_vec());
                    match result {
                        Value::Empty => Some(Value::List(Vec::with_capacity(0))),
                        _ => Some(result),
                    }
                },
                Term::name(terms) => {
                    self.traverse_terms_inner(&mut context.clone(), cache, terms)
                },
                Term::string(text) => {

                    Some(Value::Str(text.to_string()))
                },
                Term::transpose => match context.identity.take().unwrap_or(Value::Empty) {
                    Value::List(values) => {
                        trace!("transpose input {:?}", values);

                        let mut lists = Vec::new();
                        for value in values {
                            match value {
                                Value::List(v) => lists.push(v),
                                _ => {},
                            }
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
                    rawr => panic!("transpose is only supported on lists - {:?}", rawr),
                },
                Term::unsigned_number(value) => {
                    Some(Value::U64(*value))
                },
                Term::signed_number(value) => {
                    Some(Value::I64(*value))
                },
                _ => Some(self.traverse_term(context, cache, term))
            };
        }

        context.identity.take()
    }
}

impl<'a> TraversalContextImpl<'a> for StaticContext<'a> {
    fn child(&self, context: &mut TraversalContext, cache: &mut SharedCache, name: &str) {
        debug!("entered {}", name);

        let spec: Option<&FileSpec> = self.store.map(|s| s.spec_by_export(name)).flatten()
            .or_else(|| self.store.map(|s| s.spec_by_export(context.current_file.as_ref().unwrap_or(&"".to_string()))).flatten());

        if context.current_file.is_none() && spec.is_some() {
            let spec = spec.unwrap();

            // generate initial values
            let file = cache.files.entry(spec.filename.to_string()).or_insert_with(|| self.store.unwrap().file_by_filename(&spec.filename).unwrap());

            let values: Vec<Value> = (0..file.rows_count)
                .map(|i| {
                    let kv_list: Vec<Value> = spec
                        .fields
                        .iter()
                        .map(|field| {
                            Value::KeyValue(
                                Box::new(Value::Str(field.name.clone())),
                                Box::new(file.read_field(i as u64, &field)),
                            )
                        })
                        .collect();
                    Value::Object(Box::new(Value::List(kv_list)))
                })
                .collect();

            context.current_field = None;
            context.current_file = Some(spec.filename.to_string());
            context.identity = Some(Value::List(values));
        } else {
            self.enter_foreign(context, cache);
            context.current_field = Some(name.to_string());
            context.identity = Some(self.value(context));
        }
    }

    fn index(&self, context: &mut TraversalContext, index: usize) {
        let value = context.identity.take().unwrap_or(Value::Empty);
        context.identity = match value {
            Value::List(list) => match list.into_iter().nth(index) {
                Some(value) => Some(value),
                None => panic!("attempt to index outside list"),
            },
            Value::Str(str) => match str.chars().nth(index) {
                Some(value) => Some(Value::Str(value.to_string())),
                None => panic!("attempt to index outside string"),
            },
            _ => panic!("attempt to index non-indexable value {:?}", value),
        };
    }

    fn index_reverse(&self, context: &mut TraversalContext, index: usize) {
        let value = context.identity.take().unwrap_or(Value::Empty);
        context.identity = match value {
            Value::List(list) => {
                let size = list.len();
                match list.into_iter().nth(size - index) {
                    Some(value) => Some(value),
                    None => panic!("attempt to index outside list"),
                }
            },
            Value::Str(str) => match str.chars().nth(index) {
                Some(value) => Some(Value::Str(value.to_string())),
                None => panic!("attempt to index outside string"),
            },
            _ => panic!("attempt to index non-indexable value {:?}", value),
        };
    }

    fn slice(&self, context: &mut TraversalContext, from: usize, to: usize) {
        let value = context.identity.take().unwrap_or(Value::Empty);
        context.identity = match value {
            Value::List(list) => {
                let sliced = list[from..usize::min(to, list.len())].to_vec();
                Some(Value::List(sliced))
            }
            Value::Str(str) => Some(Value::Str(str[from..to].to_string())),
            _ => panic!("attempt to index non-indexable value {:?}", value),
        };
    }

    fn to_iterable(&self, context: &mut TraversalContext, cache: &mut SharedCache) -> Value {
        self.enter_foreign(context, cache);
        let value = context.identity.take().unwrap_or(Value::Empty);
        let iterable = match value {
            Value::List(list) => Value::Iterator(list),
            Value::Iterator(list) => Value::Iterator(list),
            Value::Object(content) => {
                let fields = match *content {
                    Value::List(fields) => fields,
                    _ => panic!("attempt to iterate an empty object"),
                };
                Value::Iterator(fields)
            }
            Value::Empty => Value::Iterator(Vec::with_capacity(0)),
            obj => panic!(
                "unable to iterate, should i support this? {:?}",
                obj
            ),
        };
        iterable
    }

    fn value(&self, context: &mut TraversalContext) -> Value {
        if context.identity == None {
            return Value::Empty;
        }

        match context.identity.take().unwrap() {
            // TODO: extract to function
            Value::Object(entries) => {
                match *entries {
                    Value::List(list) => {
                        let mut values = Vec::new();
                        for field in list {
                            match field {
                                Value::KeyValue(key, value) => {
                                    if *key == Value::Str(context.current_field.clone().unwrap()) {
                                        values.push(*value);
                                    }
                                },
                                _ => {}
                            }
                        }

                        values.into_iter().nth(0).unwrap_or(Value::Empty)
                    }
                    Value::KeyValue(key, value) => {
                        if *key == Value::Str(context.current_field.clone().unwrap()) {
                            *value
                        } else {
                            Value::Empty
                        }
                    }
                    _ => panic!("failed to extract value from kv! {:?}", entries),
                }
            },
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
                        },
                        Value::Object(elements) => {
                            let obj = match *elements {
                                Value::List(fields) => fields,
                                _ => panic!("uhm: {:?}", elements),
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
                                    asd => panic!("what happened? {:?}", asd),
                                }
                            }
                            first
                        },
                        val => panic!(
                            "Attempting to get field of non-iterable and non-object. {:?}",
                            val
                        ),
                    };
                    result.push(item);
                }

                Value::List(result)
            },
            Value::U64(i) => {
                let current = context.current_file.as_ref().unwrap();
                let spec = self.store.unwrap().spec(&current).unwrap();
                let file = self.store.unwrap().file_by_filename(&current).unwrap();

                // TODO: extract to function
                let kv_list: Vec<Value> = spec
                    .fields
                    .iter()
                    .map(move |field| {
                        Value::KeyValue(
                            Box::new(Value::Str(field.name.clone())),
                            Box::new(file.read_field(i, &field)),
                        )
                    })
                    .collect();

                Value::Object(Box::new(Value::List(kv_list)))
            },
            _ => return Value::Empty,
        }
    }

    fn identity(&self, context: &mut TraversalContext) -> Value {
        context.identity.clone().unwrap_or(Value::Empty)
    }

    fn enter_foreign(&self, context: &mut TraversalContext, cache: &mut SharedCache) {
        let current_spec = context
            .current_file.as_ref()
            .map(|file| self.store.unwrap().spec(&file))
            .flatten();
        let current_field = current_spec
            .map(|spec| {
                spec.fields.iter().find(|&field| {
                    context.current_field.is_some()
                        && context.current_field.clone().unwrap() == field.name
                })
            })
            .flatten();

        if current_field.is_some() && current_field.unwrap().is_foreign_key() {
            context.current_field = None;

            let value = context.identity.take().unwrap_or(Value::Empty);
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
                    item => panic!("Not a valid id for foreign key: {:?}", item),
                }
                .iter()
                .filter_map(|v| match v {
                    Value::U64(i) => Some(*i),
                    Value::List(_) => None,
                    _ => panic!("value {:?}", v),
                })
                .collect();

                let rows = self.rows_from(cache, &current_field.unwrap().file.as_ref().unwrap(), ids.as_slice());
                Some(rows)
            });

            context.current_field = None;
            context.current_file = Some(current_spec.unwrap().filename.clone());
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
                    .fields
                    .iter()
                    .map(|field| {
                        Value::KeyValue(
                            Box::new(Value::Str(field.name.clone())),
                            Box::new(file.read_field(*i, &field)),
                        )
                    })
                    .collect();
                Value::Object(Box::new(Value::List(kv_list)))
            })
            .collect();

        if values.len() > 1 {
            Value::List(values)
        } else {
            values.into_iter().nth(0).unwrap_or(Value::Empty)
        }
    }
}

// TODO: move this somewhere else
fn iterate<F>(value: Value, mut action: F) -> Value
where
    F: FnMut(Value) -> Option<Value> + Send + Sync,
{
    match value {
        Value::Iterator(elements) => {
            let mut list = Vec::new();
            for e in elements {
                match action(e) {
                    Some(value) => list.push(value),
                    _ => {}
                }
            }
            Value::List(list)
        },
        v => action(v).expect("non-iterable must return something"),
    }
}

fn reduce<F>(initial: Value, action: &mut F) -> Value
where
    F: FnMut(Value, Value) -> Value,
{
    match initial {
        Value::Iterator(elements) => {
            elements.into_iter().reduce(|accum, item| {
                action(accum, item)
            }).unwrap_or(Value::Empty)
        }
        _ => {
            action(Value::Empty, initial)
        }
    }
}
