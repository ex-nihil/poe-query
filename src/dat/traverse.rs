use log::*;
use std::collections::HashMap;
use crate::dat::file::DatFile;
use crate::DatContainer;

use super::super::lang::{Compare, Operation, Term};
use super::file::DatFileRead;
use super::reader::DatStoreImpl;
use super::specification::FieldSpecImpl;
use super::value::Value;

pub struct SharedTraversalContext<'a> {
    pub store: &'a DatContainer<'a>,
}

#[derive(Clone)]
pub struct TraversalContextEphemeral<'a> {
    pub current_field: Option<String>,
    pub current_file: Option<String>,
    pub dat_file: Option<&'a DatFile>,
    pub identity: Option<Value>,
}

pub struct SharedMutableTraversalContext {
    pub variables: HashMap<String, Value>,
    pub files: HashMap<String, DatFile>,
}



pub trait TraversalContextImpl<'a> {
    fn child(&self, local_context: &mut TraversalContextEphemeral, shared_context: &mut SharedMutableTraversalContext, name: &str);
    fn index(&self, local_context: &mut TraversalContextEphemeral, index: usize);
    fn slice(&self, local_context: &mut TraversalContextEphemeral, from: usize, to: usize);
    fn to_iterable(&self, local_context: &mut TraversalContextEphemeral, shared_context: &mut SharedMutableTraversalContext) -> Value;
    fn value(&self, local_context: &mut TraversalContextEphemeral) -> Value;
    fn identity(&self, local_context: &mut TraversalContextEphemeral) -> Value;

    fn enter_foreign(&self, local_context: &mut TraversalContextEphemeral, shared_context: &mut SharedMutableTraversalContext);
    fn rows_from(&self, shared_context: &mut SharedMutableTraversalContext, file: &str, indices: &[u64]) -> Value;
}

pub trait TermsProcessor {
    fn process(&self, local_context: &mut TraversalContextEphemeral, shared_context: &mut SharedMutableTraversalContext, parsed_terms: &[Term]) -> Value;
    fn traverse_term(&self, local_context: &mut TraversalContextEphemeral, shared_context: &mut SharedMutableTraversalContext, term: &Term) -> Value;
    fn traverse_terms_inner(&self, local_context: &mut TraversalContextEphemeral, shared_context: &mut SharedMutableTraversalContext, terms: &[Term]) -> Option<Value>;
}

impl TermsProcessor for SharedTraversalContext<'_> {
    fn process(&self, local_context: &mut TraversalContextEphemeral, shared_context: &mut SharedMutableTraversalContext, parsed_terms: &[Term]) -> Value {
        let values: Vec<Value> = if parsed_terms.contains(&Term::comma) {
            parsed_terms
                .split(|term| match term {
                    Term::comma => true,
                    _ => false,
                })
                .filter_map(|terms| self.traverse_terms_inner(&mut local_context.clone(), shared_context, &terms))
                .collect()
        } else {
            vec![self
                .traverse_terms_inner(local_context, shared_context, parsed_terms)
                .unwrap_or(Value::Empty)]
        };

        local_context.identity = if values.len() > 1 {
            Some(Value::List(values))
        } else if values.len() == 1 {
            Some(values.first().unwrap().clone())
        } else {
            None
        };

        self.identity(local_context)
    }

    fn traverse_term(&self, local_context: &mut TraversalContextEphemeral, shared_context: &mut SharedMutableTraversalContext, term: &Term) -> Value {
        match term {
            Term::by_name(key) => {
                self.child(local_context, shared_context, key);
                self.identity(local_context)
            }
            Term::by_index(i) => {
                self.index(local_context, *i);
                self.identity(local_context)
            }
            Term::slice(from, to) => {
                self.slice(local_context, *from, *to);
                self.identity(local_context)
            }
            _ => panic!("unhandled term: {:?}", term),
        }
    }

    // Comma has be dealt with
    fn traverse_terms_inner(&self, local_context: &mut TraversalContextEphemeral, shared_context: &mut SharedMutableTraversalContext, terms: &[Term]) -> Option<Value> {
        let mut new_value = None;
        for term in terms {
            match term {
                Term::select(lhs, op, rhs) => {
                    let elems = self.to_iterable(local_context, shared_context);
                    let result = iterate(&elems, |v| {
                        let left = SharedTraversalContext::process(self, &mut TraversalContextEphemeral {
                            current_field: local_context.current_field.clone(),
                            current_file: local_context.current_file.clone(),
                            dat_file: local_context.dat_file,
                            identity: Some(v.clone()),
                        }, shared_context,  lhs);

                        let right = SharedTraversalContext::process(self, &mut TraversalContextEphemeral {
                            current_field: local_context.current_field.clone(),
                            current_file: local_context.current_file.clone(),
                            dat_file: local_context.dat_file,
                            identity: Some(v.clone()),
                        }, shared_context, rhs);

                        let selected = match op {
                            Compare::equals => left == right,
                            Compare::not_equals => left != right,
                            Compare::less_than => left < right,
                            Compare::greater_than => left > right,
                        };
                        if selected {
                            Some(v.clone())
                        } else {
                            None
                        }
                    });
                    new_value = Some(result);
                }
                Term::noop => {}
                Term::iterator => {
                    new_value = Some(self.to_iterable(local_context, shared_context));
                }
                Term::calculate(lhs, op, rhs) => {
                    let lhs_result = self.clone().traverse_terms_inner(&mut local_context.clone(), shared_context, lhs);
                    let rhs_result = self.clone().traverse_terms_inner(&mut local_context.clone(), shared_context, rhs);
                    let result = match op {
                        // TODO: add operation support on the different types
                        Operation::add => lhs_result.unwrap() + rhs_result.unwrap(),
                        _ => Value::Empty,
                    };
                    new_value = Some(result);
                }
                Term::set_variable(name) => {
                    shared_context.variables
                        .insert(name.to_string(), self.identity(local_context).clone());
                }
                Term::get_variable(name) => {
                    new_value = Some(shared_context.variables.get(name).unwrap_or(&Value::Empty).clone());
                }
                Term::reduce(outer_terms, init, terms) => {
                    // seach for variables
                    let vars: Vec<&String> = outer_terms
                        .iter()
                        .filter_map(|term| match term {
                            Term::set_variable(variable) => Some(variable),
                            _ => None,
                        })
                        .collect();
                    self.traverse_terms_inner(local_context, shared_context, outer_terms);


                    let initial = self.traverse_terms_inner(&mut TraversalContextEphemeral {
                        current_field: local_context.current_field.clone(),
                        current_file: local_context.current_file.clone(),
                        dat_file: local_context.dat_file,
                        identity: None
                    }, shared_context, init);
                    let variable = vars.first().unwrap().as_str();
                    let value = shared_context
                        .variables
                        .get(variable)
                        .unwrap_or(&Value::Empty)
                        .clone();

                    local_context.identity = initial;
                    let result = reduce(&value, &mut |v| {
                        shared_context.variables.insert(variable.to_string(), v.clone());
                        local_context.identity = Some(self.process(local_context, shared_context, terms));
                        local_context.identity.clone()
                    });
                    new_value = Some(result);
                }
                Term::map(terms) => {
                    let result = iterate(&self.to_iterable(local_context, shared_context), |v| {
                        Some(self.process(&mut TraversalContextEphemeral {
                            current_field: local_context.current_field.clone(),
                            current_file: local_context.current_file.clone(),
                            dat_file: local_context.dat_file,
                            identity: Some(v.clone())
                        }, shared_context, terms))
                    });
                    new_value = Some(result);
                }
                Term::object(obj_terms) => {
                    if let Some(value) = &local_context.identity {
                        new_value = Some(iterate(value, |v| {
                            let output = self.process(&mut TraversalContextEphemeral {
                                current_field: local_context.current_field.clone(),
                                current_file: local_context.current_file.clone(),
                                dat_file: local_context.dat_file,
                                identity: Some(v.clone())
                            }, shared_context, obj_terms);
                            Some(Value::Object(Box::new(output)))
                        }));
                    } else {
                        let output = self.process(&mut local_context.clone(), shared_context, obj_terms);
                        new_value = Some(Value::Object(Box::new(output)));
                    }
                }
                Term::kv(key, value_terms) => {
                    let key = self.process(&mut local_context.clone(), shared_context, &vec![*key.clone()]);
                    let result = self.process(&mut local_context.clone(), shared_context, &value_terms.to_vec());
                    match key {
                        Value::Empty => {}
                        Value::List(_) => {}
                        _ => {
                            new_value = Some(Value::KeyValue(Box::new(key), Box::new(result)));
                        }
                    }
                }
                Term::identity => {
                    if local_context.current_file.is_none() && local_context.identity.is_none() {
                        let exports: Vec<Value> = self
                            .store
                            .exports()
                            .iter()
                            .map(|export| {
                                let spec = self.store.spec_by_export(export).unwrap();

                                Value::KeyValue(
                                    Box::new(Value::Str(spec.export.to_string())),
                                    Box::new(Value::List(vec![])),
                                )
                            })
                            .collect();
                        new_value = Some(Value::Object(Box::new(Value::List(exports))));
                    } else {
                        new_value = Some(self.identity(local_context));
                    }
                }
                Term::array(arr_terms) => {
                    let result = self.process(&mut local_context.clone(), shared_context, &arr_terms.to_vec());
                    new_value = match result {
                        Value::Empty => Some(Value::List(Vec::with_capacity(0))),
                        _ => Some(result),
                    };
                }
                Term::name(terms) => {
                    new_value = self.traverse_terms_inner(&mut local_context.clone(), shared_context, terms);
                }
                Term::string(text) => {
                    new_value = Some(Value::Str(text.to_string()));
                }
                Term::transpose => match local_context.identity.as_ref().unwrap_or(&Value::Empty) {
                    Value::List(values) => {
                        let lists: Vec<Vec<Value>> = values
                            .iter()
                            .filter_map(|value| match value {
                                Value::List(v) => Some(v.clone()),
                                _ => None,
                            })
                            .collect();

                        let max = lists
                            .iter()
                            .fold(0u64, |max, list| u64::max(max, list.len() as u64));

                        let outer: Vec<Value> = (0..max)
                            .map(|i| {
                                let inner = lists
                                    .iter()
                                    .map(|list| {
                                        list.get(i as usize).unwrap_or(&Value::Empty).clone()
                                    })
                                    .collect();
                                Value::List(inner)
                            })
                            .collect();
                        new_value = Some(Value::List(outer));
                    }
                    rawr => panic!("transpose is only supported on lists - {:?}", rawr),
                },
                Term::unsigned_number(value) => {
                    new_value = Some(Value::U64(*value));
                }
                Term::signed_number(value) => {
                    new_value = Some(Value::I64(*value));
                }
                _ => {
                    new_value = Some(self.traverse_term(local_context, shared_context, &term.clone()));
                }
            }

            trace!("identity updated: {:?}", new_value);
            local_context.identity = new_value.clone(); // yikes
        }

        new_value
    }
}

impl<'a> TraversalContextImpl<'a> for SharedTraversalContext<'a> {
    fn child(&self, local_context: &mut TraversalContextEphemeral, shared_context: &mut SharedMutableTraversalContext, name: &str) {
        debug!("child {}", name);
        let spec = self.store.spec_by_export(name);
        if local_context.current_file.is_none() && spec.is_some() {
            let spec = spec.unwrap();

            // generate initial values
            let file = shared_context.files.entry(spec.filename.to_string()).or_insert_with(|| self.store.file(&spec.filename).unwrap());

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

            local_context.current_field = None;
            local_context.current_file = Some(spec.filename.to_string());
            local_context.identity = Some(Value::List(values));
        } else {
            self.enter_foreign(local_context, shared_context);
            local_context.current_field = Some(name.to_string());
            local_context.identity = Some(self.value(local_context));
        }
    }

    fn index(&self, local_context: &mut TraversalContextEphemeral, index: usize) {
        let value = local_context.identity.as_ref().unwrap_or(&Value::Empty);
        match value {
            Value::List(list) => match list.get(index) {
                Some(value) => local_context.identity = Some(value.clone()),
                None => panic!("attempt to index outside list"),
            },
            Value::Str(str) => match str.chars().nth(index) {
                Some(value) => local_context.identity = Some(Value::Str(value.to_string())),
                None => panic!("attempt to index outside string"),
            },
            _ => panic!("attempt to index non-indexable value {:?}", value),
        }
    }

    fn slice(&self, local_context: &mut TraversalContextEphemeral, from: usize, to: usize) {
        let value = local_context.identity.as_ref().unwrap_or(&Value::Empty);
        match value {
            Value::List(list) => {
                local_context.identity = Some(Value::List(list[from..usize::min(to, list.len())].to_vec()))
            }
            Value::Str(str) => local_context.identity = Some(Value::Str(str[from..to].to_string())),
            _ => panic!("attempt to index non-indexable value {:?}", value),
        }
    }

    fn to_iterable(&self, local_context: &mut TraversalContextEphemeral, shared_context: &mut SharedMutableTraversalContext) -> Value {
        self.enter_foreign(local_context, shared_context);
        let value = local_context.identity.as_ref().unwrap_or(&Value::Empty);
        let iteratable = match value {
            Value::List(list) => Value::Iterator(list.clone()),
            Value::Iterator(list) => Value::Iterator(list.clone()),
            Value::Object(content) => {
                let fields = match &**content {
                    Value::List(fields) => fields,
                    _ => panic!("attempt to iterate an empty object"),
                };
                Value::Iterator(fields.clone())
            }
            Value::Empty => Value::Iterator(Vec::with_capacity(0)),
            obj => panic!(
                "unable to iterate, should i support this? {:?}",
                obj
            ),
        };
        iteratable
    }

    fn value(&self, local_context: &mut TraversalContextEphemeral) -> Value {
        if local_context.identity == None {
            return Value::Empty;
        }

        match local_context.identity.as_ref().unwrap() {
            // TODO: extract to function
            Value::Object(entries) => {
                let v = match &**entries {
                    Value::List(list) => {
                        let values: Vec<Value> = list
                            .iter()
                            .filter_map(|field| match field {
                                Value::KeyValue(key, value) => {
                                    if **key == Value::Str(local_context.current_field.clone().unwrap()) {
                                        Some(*value.clone())
                                    } else {
                                        None
                                    }
                                }
                                _ => panic!("failed to extract value from kv"),
                            })
                            .collect();

                        values.first().unwrap_or(&Value::Empty).clone()
                    }
                    Value::KeyValue(key, value) => {
                        if **key == Value::Str(local_context.current_field.clone().unwrap()) {
                            *value.clone()
                        } else {
                            Value::Empty
                        }
                    }
                    _ => panic!("failed to extract value from kv! {:?}", entries),
                };
                return v.clone();
            }
            Value::Iterator(values) => {
                let result: Vec<Value> = values
                    .iter()
                    .map(|value| match value {
                        Value::KeyValue(k, v) => {
                            if Value::Str(local_context.current_field.clone().unwrap()) == **k {
                                *v.clone()
                            } else {
                                Value::Empty
                            }
                        }
                        Value::Object(elements) => {
                            let obj = match *elements.clone() {
                                Value::List(fields) => fields,
                                _ => panic!("uhm: {:?}", elements),
                            };
                            obj.iter()
                                .filter_map(|field| match field {
                                    Value::KeyValue(k, v) => {
                                        if Value::Str(local_context.current_field.clone().unwrap()) == **k {
                                            Some(*v.clone())
                                        } else {
                                            None
                                        }
                                    }
                                    asd => panic!("what happened? {:?}", asd),
                                })
                                .collect::<Vec<Value>>()
                                .first()
                                .unwrap_or(&Value::Empty)
                                .clone()
                        }
                        val => panic!(
                            "Attempting to get field of non-iterable and non-object. {:?}",
                            val
                        ),
                    })
                    .collect();

                return Value::List(result);
            }
            Value::U64(i) => {
                let current = local_context.current_file.as_ref().unwrap();
                let spec = self.store.spec(&current).unwrap();
                let file = self.store.file(&current).unwrap();

                // TODO: extract to function
                let kv_list: Vec<Value> = spec
                    .fields
                    .iter()
                    .map(move |field| {
                        Value::KeyValue(
                            Box::new(Value::Str(field.name.clone())),
                            Box::new(file.read_field(*i, &field)),
                        )
                    })
                    .collect();

                return Value::Object(Box::new(Value::List(kv_list)));
            }
            _ => return Value::Empty,
        };
    }

    fn identity(&self, local_context: &mut TraversalContextEphemeral) -> Value {
        local_context.identity.clone().unwrap_or(Value::Empty)
    }

    fn enter_foreign(&self, local_context: &mut TraversalContextEphemeral, shared_context: &mut SharedMutableTraversalContext) {
        let current_spec = local_context
            .current_file.as_ref()
            .map(|file| self.store.spec(&file))
            .flatten();
        let current_field = current_spec
            .map(|spec| {
                spec.fields.iter().find(|&field| {
                    local_context.current_field.is_some()
                        && local_context.current_field.clone().unwrap() == field.name
                })
            })
            .flatten();

        if current_field.is_some() && current_field.unwrap().is_foreign_key() {
            local_context.current_field = None;

            let value = local_context.identity.clone().unwrap_or(Value::Empty);
            let value = match value {
                Value::List(items) => Value::Iterator(items.clone()),
                _ => value,
            };

            let result = iterate(&value, |v| {
                let ids: Vec<u64> = match v {
                    Value::List(ids) => ids.clone(),     // TODO: yikes
                    Value::Iterator(ids) => ids.clone(), // TODO: yikes
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

                let rows = self.rows_from(shared_context, &current_field.unwrap().file, ids.as_slice());
                Some(rows)
            });

            local_context.current_field = None;
            local_context.current_file = Some(current_spec.unwrap().filename.clone());
            local_context.identity = Some(result);
        }
    }

    fn rows_from(&self, shared_context: &mut SharedMutableTraversalContext, filepath: &str, indices: &[u64]) -> Value {
        let foreign_spec = self.store.spec(filepath).unwrap();

        let file = shared_context.files.entry(filepath.to_string()).or_insert_with(|| self.store.file(filepath).unwrap());

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
            values.first().unwrap_or(&Value::Empty).clone()
        }
    }
}

// TODO: move this somewhere else
fn iterate<F>(value: &Value, mut action: F) -> Value
where
    F: FnMut(Value) -> Option<Value> + Send + Sync,
{
    match value {
        Value::Iterator(elements) => Value::List(
            elements
                .iter()
                .filter_map(|e| action(e.clone()))
                .collect(),
        ),
        _ => action(value.clone()).expect("non-iterable must return something"),
    }
}

fn reduce<F>(value: &Value, action: &mut F) -> Value
where
    F: FnMut(&Value) -> Option<Value>,
{
    let mut result = Value::Empty;
    match value {
        Value::Iterator(elements) => {
            elements.iter().for_each(|e| {
                result = action(e).expect("reduce operation must return a value");
            });
        }
        _ => {
            result = action(value).expect("reduce operation must return a value");
        }
    }
    result
}
