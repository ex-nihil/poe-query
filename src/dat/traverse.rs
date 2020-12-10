use std::collections::HashMap;

use super::super::lang::{Compare, Operation, Term};
use super::file::DatFileRead;
use super::reader::DatStore;
use super::reader::DatStoreImpl;
use super::specification::FieldSpecImpl;
use super::value::Value;

#[derive(Debug)]
pub struct TraversalContext<'a> {
    pub store: DatStore<'a>,
    pub variables: HashMap<String, Value>,
    pub current_field: Option<String>,
    pub current_file: Option<&'a str>,
    pub identity: Option<Value>,
}

pub trait TraversalContextImpl {
    fn child(&mut self, name: &str);
    fn index(&mut self, index: usize);
    fn slice(&mut self, from: usize, to: usize);
    fn to_iterable(&mut self) -> Value;
    fn value(&mut self) -> Value;
    fn identity(&self) -> Value;
    fn clone(&self) -> TraversalContext;

    fn enter_foreign(&mut self);
    fn rows_from(&self, file: &str, indices: &[u64]) -> Value;
    fn clone_with_value(&self, name: Option<Value>) -> TraversalContext;
}
pub trait TermsProcessor {
    fn process(&mut self, parsed_terms: &Vec<Term>) -> Value;
    fn traverse_term(&mut self, term: &Term) -> Value;
    fn traverse_terms_inner(&mut self, parsed_terms: &[Term]) -> Option<Value>;
}

impl TermsProcessor for TraversalContext<'_> {

    fn process(&mut self, parsed_terms: &Vec<Term>) -> Value {
        let parts = parsed_terms.split(|term| match term {
            Term::comma => true,
            _ => false,
        });

        let values: Vec<Value> = parts
            .filter_map(|terms| self.clone().traverse_terms_inner(&terms))
            .collect();

        self.identity = if values.len() > 1 {
            Some(Value::List(values))
        } else if values.len() == 1 {
            Some(values.first().unwrap().clone())
        } else {
            None
        };

        self.identity()
    }

    // Comma has be dealt with
    fn traverse_terms_inner(&mut self, terms: &[Term]) -> Option<Value> {
        let mut new_value = None;
        for term in terms {
            match term {
                Term::select(lhs, op, rhs) => {
                    let elems = self.to_iterable();
                    let result = iterate(&elems, |v| {
                        let mut clone = self.clone_with_value(Some(v.clone()));
                        let left = clone.process(lhs);
                        clone = self.clone_with_value(Some(v.clone()));
                        let right = clone.process(rhs);
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
                    new_value = Some(self.to_iterable());
                }
                Term::calculate(lhs, op, rhs) => {
                    let lhs_result = self.clone().traverse_terms_inner(lhs);
                    let rhs_result = self.clone().traverse_terms_inner(rhs);
                    let result = match op {
                        // TODO: add operation support on the different types
                        Operation::add => lhs_result.unwrap() + rhs_result.unwrap(),
                        _ => Value::Empty,
                    };
                    new_value = Some(result);
                }
                Term::set_variable(name) => {
                    self.variables
                        .insert(name.to_string(), self.identity().clone());
                }
                Term::get_variable(name) => {
                    new_value = Some(self.variables.get(name).unwrap_or(&Value::Empty).clone());
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
                    self.traverse_terms_inner(outer_terms);

                    let initial = self.clone_with_value(None).traverse_terms_inner(init);
                    let variable = vars.first().unwrap().as_str();
                    let value = self
                        .variables
                        .get(variable)
                        .unwrap_or(&Value::Empty)
                        .clone();

                    self.identity = initial;
                    let result = reduce(&value, &mut |v| {
                        self.variables.insert(variable.to_string(), v.clone());
                        self.identity = Some(self.process(terms));
                        self.identity.clone()
                    });
                    new_value = Some(result);
                }
                Term::map(terms) => {
                    let result = iterate(&self.to_iterable(), |v| {
                        Some(self.clone_with_value(Some(v.clone())).process(terms))
                    });
                    new_value = Some(result);
                }
                Term::object(obj_terms) => {
                    if let Some(value) = &self.identity {
                        new_value = Some(iterate(value, |v| {
                            let mut clone = self.clone_with_value(Some(v.clone()));
                            let output = clone.process(obj_terms);
                            Some(Value::Object(Box::new(output)))
                        }));
                    } else {
                        let output = self.clone().process(obj_terms);
                        new_value = Some(Value::Object(Box::new(output)));
                    }
                }
                Term::kv(key_terms, value_terms) => {
                    let key = self.clone().process(&key_terms.to_vec());
                    let result = self.clone().process(&value_terms.to_vec());
                    new_value = Some(Value::KeyValue(Box::new(key), Box::new(result)));
                }
                Term::identity => {
                    if self.current_file.is_none() && self.identity.is_none() {
                        let exports: Vec<Value> = self
                            .store
                            .exports()
                            .iter()
                            .map(|export| {
                                let spec = self.store.spec_by_export(export).unwrap();
                                let file = self.store.file(&spec.filename).unwrap();

                                Value::KeyValue(
                                    Box::new(Value::Str(spec.export.to_string())),
                                    Box::new(Value::List(vec![Value::Str(format!(
                                        "list containing {} rows",
                                        file.rows_count
                                    ))])),
                                )
                            })
                            .collect();
                        new_value = Some(Value::Object(Box::new(Value::List(exports))));
                    } else {
                        new_value = Some(self.identity());
                    }
                }
                Term::array(arr_terms) => {
                    let result = self.clone().process(&arr_terms.to_vec());
                    new_value = match result {
                        Value::Empty => Some(Value::List(Vec::with_capacity(0))),
                        _ => Some(result),
                    };
                }
                Term::name(terms) => {
                    new_value = self.clone().traverse_terms_inner(terms);
                }
                Term::string(text) => {
                    new_value = Some(Value::Str(text.to_string()));
                }
                Term::transpose => match self.identity.clone().unwrap_or(Value::Empty) {
                    Value::List(values) => {
                        let lists: Vec<Vec<Value>> = values
                            .iter()
                            .map(|value| match value {
                                Value::List(v) => v.clone(),
                                rawr => panic!(format!(
                                    "transpose is only supported on lists + {:?}",
                                    rawr
                                )),
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
                    rawr => panic!(format!("transpose is only supported on lists - {:?}", rawr)),
                },
                Term::unsigned_number(value) => {
                    new_value = Some(Value::U64(*value));
                }
                Term::signed_number(value) => {
                    new_value = Some(Value::I64(*value));
                }
                _ => {
                    new_value = Some(self.traverse_term(&term.clone()));
                }
            }

            self.identity = new_value.clone(); // yikes
        }

        new_value
    }

    fn traverse_term(&mut self, term: &Term) -> Value {
        match term {
            Term::by_name(key) => {
                self.child(key);
                self.identity()
            }
            Term::by_index(i) => {
                self.index(*i);
                self.identity()
            }
            Term::slice(from, to) => {
                self.slice(*from, *to);
                self.identity()
            }
            _ => panic!(format!("unhandled term: {:?}", term)),
        }
    }
}

impl TraversalContextImpl for TraversalContext<'_> {
    fn child(&mut self, name: &str) {
        let spec = self.store.spec_by_export(name);
        if self.current_file.is_none() && spec.is_some() {
            let spec = spec.unwrap();

            // generate initial values
            let file = self.store.file(&spec.filename).unwrap();
            let values: Vec<Value> = (0..file.rows_count)
                .map(|i| {
                    let kv_list: Vec<Value> = spec
                        .fields
                        .iter()
                        .map(move |field| {
                            Value::KeyValue(
                                Box::new(Value::Str(field.name.clone())),
                                Box::new(file.read_field(i as u64, &field)),
                            )
                        })
                        .collect();
                    Value::Object(Box::new(Value::List(kv_list)))
                })
                .collect();

            self.current_field = None;
            self.current_file = Some(&spec.filename);
            self.identity = Some(Value::List(values));
        } else {
            self.enter_foreign();
            self.current_field = Some(name.to_string());
            self.identity = Some(self.value());
        }
    }

    fn enter_foreign(&mut self) {
        let current_spec = self
            .current_file
            .map(|file| self.store.spec(file))
            .flatten();
        let current_field = current_spec
            .map(|spec| {
                spec.fields.iter().find(|&field| {
                    self.current_field.is_some()
                        && self.current_field.clone().unwrap() == field.name
                })
            })
            .flatten();

        if current_field.is_some() && current_field.unwrap().is_foreign_key() {
            self.current_field = None;

            let value = self.identity.clone().unwrap_or(Value::Empty);
            let value = match value {
                Value::List(items) => Value::Iterator(items),
                _ => value,
            };

            let result = iterate(&value, |v| {
                let ids: Vec<u64> = match v {
                    Value::List(ids) => ids.clone(),     // TODO: yikes
                    Value::Iterator(ids) => ids.clone(), // TODO: yikes
                    Value::U64(id) => vec![Value::U64(*id)],
                    Value::Empty => vec![],
                    item => panic!(format!("Not a valid id for foreign key: {:?}", item)),
                }
                .iter()
                .filter_map(|v| match v {
                    Value::U64(i) => Some(*i),
                    Value::List(_) => None,
                    _ => panic!(format!("value {:?}", v)),
                })
                .collect();

                let rows = self.rows_from(&current_field.unwrap().file, ids.as_slice());
                Some(rows)
            });

            self.current_field = None;
            self.current_file = Some(current_spec.unwrap().filename.as_str());
            self.identity = Some(result);
        }
    }

    fn to_iterable(&mut self) -> Value {
        self.enter_foreign();
        let value = self.identity.clone().unwrap_or(Value::Empty);
        let iteratable = match value {
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
            obj => panic!(format!(
                "unable to iterate, should i support this? {:?}",
                obj
            )),
        };
        iteratable
    }

    fn slice(&mut self, from: usize, to: usize) {
        let value = self.identity.clone().unwrap_or(Value::Empty);
        match value {
            Value::List(list) => {
                self.identity = Some(Value::List(list[from..usize::min(to, list.len())].to_vec()))
            }
            Value::Str(str) => self.identity = Some(Value::Str(str[from..to].to_string())),
            _ => panic!("attempt to index non-indexable value {:?}", value),
        }
    }

    fn index(&mut self, index: usize) {
        let value = self.identity.clone().unwrap_or(Value::Empty);
        match value {
            Value::List(list) => match list.get(index) {
                Some(value) => self.identity = Some(value.clone()),
                None => panic!("attempt to index outside list"),
            },
            Value::Str(str) => match str.chars().nth(index) {
                Some(value) => self.identity = Some(Value::Str(value.to_string())),
                None => panic!("attempt to index outside string"),
            },
            _ => panic!("attempt to index non-indexable value {:?}", value),
        }
    }

    fn identity(&self) -> Value {
        self.identity.clone().unwrap_or(Value::Empty)
    }

    fn value(&mut self) -> Value {
        let identity = self.identity.clone().unwrap_or(Value::Empty);
        match identity {
            // TODO: extract to function
            Value::Object(entries) => {
                let v = match *entries {
                    Value::List(list) => {
                        let values: Vec<Value> = list
                            .iter()
                            .filter_map(|field| match field {
                                Value::KeyValue(key, value) => {
                                    if **key == Value::Str(self.current_field.clone().unwrap()) {
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
                        if *key == Value::Str(self.current_field.clone().unwrap()) {
                            *value.clone()
                        } else {
                            Value::Empty
                        }
                    }
                    _ => panic!(format!("failed to extract value from kv! {:?}", entries)),
                };
                return v.clone();
            }
            Value::Iterator(values) => {
                let result: Vec<Value> = values
                    .iter()
                    .map(|value| match value {
                        Value::KeyValue(k, v) => {
                            if Value::Str(self.current_field.clone().unwrap()) == **k {
                                *v.clone()
                            } else {
                                Value::Empty
                            }
                        }
                        Value::Object(elements) => {
                            let obj = match *elements.clone() {
                                Value::List(fields) => fields,
                                _ => panic!(format!("uhm: {:?}", elements)),
                            };
                            obj.iter()
                                .filter_map(|field| match field {
                                    Value::KeyValue(k, v) => {
                                        if Value::Str(self.current_field.clone().unwrap()) == **k {
                                            Some(*v.clone())
                                        } else {
                                            None
                                        }
                                    }
                                    asd => panic!(format!("what happened? {:?}", asd)),
                                })
                                .collect::<Vec<Value>>()
                                .first()
                                .unwrap_or(&Value::Empty)
                                .clone()
                        }
                        val => panic!(format!(
                            "Attempting to get field of non-iterable and non-object. {:?}",
                            val
                        )),
                    })
                    .collect();

                return Value::List(result);
            }
            Value::U64(i) => {
                let current = self.current_file.unwrap();
                let spec = self.store.spec(current).unwrap();
                let file = self.store.file(current).unwrap();

                // TODO: extract to function
                let kv_list: Vec<Value> = spec
                    .fields
                    .iter()
                    .map(move |field| {
                        Value::KeyValue(Box::new(Value::Str(field.name.clone())), Box::new(file.read_field(i, &field)))
                    })
                    .collect();

                return Value::Object(Box::new(Value::List(kv_list)));
            }
            _ => return Value::Empty,
        };
    }

    fn clone(&self) -> TraversalContext {
        TraversalContext {
            store: self.store,
            variables: self.variables.clone(),
            current_field: self.current_field.clone(),
            current_file: self.current_file,
            identity: self.identity.clone(),
        }
    }

    fn rows_from(&self, filepath: &str, indices: &[u64]) -> Value {
        let foreign_spec = self.store.spec(filepath).unwrap();
        let file = self.store.file(filepath).unwrap();

        let values: Vec<Value> = indices
            .iter()
            .map(|i| {
                let kv_list: Vec<Value> = foreign_spec
                    .fields
                    .iter()
                    .map(move |field| {
                        Value::KeyValue(Box::new(Value::Str(field.name.clone())), Box::new(file.read_field(*i, &field)))
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
    // current value can be large datasets
    fn clone_with_value(&self, value: Option<Value>) -> TraversalContext {
        TraversalContext {
            store: self.store,
            variables: self.variables.clone(),
            current_field: self.current_field.clone(),
            current_file: self.current_file,
            identity: value,
        }
    }
}

// TODO: move this somewhere else
fn iterate<F>(value: &Value, action: F) -> Value
where
    F: Fn(&Value) -> Option<Value>,
{
    match value {
        Value::Iterator(elements) => {
            Value::List(elements.iter().filter_map(|e| action(e)).collect())
        }
        _ => action(value).expect("non-iterable must return something"),
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
