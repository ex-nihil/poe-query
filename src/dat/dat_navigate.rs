use std::collections::HashMap;
use std::convert::TryFrom;

use super::super::lang::{Compare, Term};
use super::dat_file::DatFileRead;
use super::dat_file::{DatFile, DatValue};
use super::dat_reader::DatStore;
use super::dat_reader::DatStoreImpl;
use super::dat_spec::FieldSpecImpl;
use super::dat_spec::FileSpec;

#[derive(Debug)]
pub struct TraversalContext<'a> {
    pub store: DatStore<'a>,
    pub variables: HashMap<String, DatValue>,
    pub current_field: Option<String>, // I should be able to get rid of this
    pub current_file: Option<&'a str>,
    pub identity: Option<DatValue>,
}

pub trait TraversalContextImpl {
    fn child(&mut self, name: &str);
    fn index(&mut self, index: usize);
    fn slice(&mut self, from: usize, to: usize);
    fn to_iterable(&mut self) -> DatValue;
    fn value(&mut self) -> DatValue;
    fn identity(&self) -> DatValue;
    fn clone(&self) -> TraversalContext;

    fn enter_foreign(&mut self);
    fn rows_from(&self, file: &str, indices: &[u64]) -> DatValue;
    fn clone_with_value(&self, name: Option<DatValue>) -> TraversalContext;
}
pub trait TermsProcessor {
    fn process(&mut self, parsed_terms: &Vec<Term>) -> DatValue;
    fn traverse_term(&mut self, term: &Term) -> DatValue;
    fn traverse_terms_inner(&mut self, parsed_terms: &[Term]) -> Option<DatValue>;
}

impl TermsProcessor for TraversalContext<'_> {
    fn process(&mut self, parsed_terms: &Vec<Term>) -> DatValue {
        let parts = parsed_terms.split(|term| match term {
            Term::comma => true,
            _ => false,
        });

        let values: Vec<DatValue> = parts
            .filter_map(|terms| self.clone().traverse_terms_inner(&terms))
            .collect();

        self.identity = if values.len() > 1 {
            Some(DatValue::List(values))
        } else if values.len() == 1 {
            Some(values.first().unwrap().clone())
        } else {
            None
        };

        self.identity()
    }

    // Comma has be dealt with
    fn traverse_terms_inner(&mut self, terms: &[Term]) -> Option<DatValue> {
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
                Term::iterator => {
                    new_value = Some(self.to_iterable());
                }
                Term::set_variable(name) => {
                    self.variables
                        .insert(name.to_string(), self.identity().clone());
                }
                Term::get_variable(name) => {
                    new_value = Some(self.variables.get(name).unwrap_or(&DatValue::Empty).clone());
                }
                Term::reduce(init, terms) => {
                    // seach for variables
                    let vars: Vec<&String> = terms
                        .iter()
                        .filter_map(|term| match term {
                            Term::get_variable(variable) => Some(variable),
                            _ => None,
                        })
                        .collect();

                    if vars.is_empty() {
                        let initial = self.traverse_terms_inner(&[*init.clone()]);
                        let result = self.clone_with_value(initial).process(terms);
                        new_value = Some(result);
                    } else {
                        let initial = self.traverse_terms_inner(&[*init.clone()]);
                        let variable = vars.first().unwrap().as_str();
                        let value = self.variables.get(variable).unwrap_or(&DatValue::Empty).clone();

                        self.identity = initial;
                        let result = reduce(&value, &mut |v| {
                            self.variables.insert(variable.to_string(), v.clone());
                            self.identity = Some(self.process(terms));
                            self.identity.clone()
                        });
                        new_value = Some(result);
                    }
                }
                Term::object(obj_terms) => {
                    if let Some(value) = &self.identity {
                        new_value = Some(iterate(value, |v| {
                            let mut clone = self.clone_with_value(Some(v.clone()));
                            let output = clone.process(obj_terms);
                            Some(DatValue::Object(Box::new(output)))
                        }));
                    } else {
                        let output = self.clone().process(obj_terms);
                        new_value = Some(DatValue::Object(Box::new(output)));
                    }
                }
                Term::kv(key, kv_terms) => {
                    let result = self.clone().process(&kv_terms.to_vec());
                    new_value = Some(DatValue::KeyValue(key.to_string(), Box::new(result)));
                }
                Term::identity => {
                    if self.current_file.is_none() && self.identity.is_none() {
                        let exports: Vec<DatValue> = self
                            .store
                            .exports()
                            .iter()
                            .map(|export| {
                                let spec = self.store.spec_by_export(export).unwrap();
                                let file = self.store.file(&spec.filename).unwrap();

                                DatValue::KeyValue(
                                    spec.export.to_string(),
                                    Box::new(DatValue::List(vec![DatValue::Str(format!(
                                        "list containing {} rows",
                                        file.rows_count
                                    ))])),
                                )
                            })
                            .collect();
                        new_value = Some(DatValue::Object(Box::new(DatValue::List(exports))));
                    } else {
                        new_value = Some(self.identity());
                    }
                }
                Term::array(arr_terms) => {
                    let result = self.clone().process(&arr_terms.to_vec());
                    new_value = match result {
                        DatValue::Empty => Some(DatValue::List(Vec::with_capacity(0))),
                        _ => Some(result),
                    };
                }
                Term::string(text) => {
                    new_value = Some(DatValue::Str(text.to_string()));
                }
                Term::transpose => match self.identity.clone().unwrap_or(DatValue::Empty) {
                    DatValue::List(values) => {
                        let lists: Vec<Vec<DatValue>> = values
                            .iter()
                            .map(|value| match value {
                                DatValue::List(v) => v.clone(),
                                rawr => panic!(format!(
                                    "transpose is only supported on lists + {:?}",
                                    rawr
                                )),
                            })
                            .collect();

                        let max = lists
                            .iter()
                            .fold(0u64, |max, list| u64::max(max, list.len() as u64));

                        let outer: Vec<DatValue> = (0..max)
                            .map(|i| {
                                let inner = lists
                                    .iter()
                                    .map(|list| {
                                        list.get(i as usize).unwrap_or(&DatValue::Empty).clone()
                                    })
                                    .collect();
                                DatValue::List(inner)
                            })
                            .collect();
                        new_value = Some(DatValue::List(outer));
                    }
                    rawr => panic!(format!("transpose is only supported on lists - {:?}", rawr)),
                },
                Term::unsigned_number(value) => {
                    new_value = Some(DatValue::U64(*value));
                }
                Term::signed_number(value) => {
                    new_value = Some(DatValue::I64(*value));
                }
                _ => {
                    new_value = Some(self.traverse_term(&term.clone()));
                }
            }

            self.identity = new_value.clone(); // yikes
        }

        new_value
    }

    fn traverse_term(&mut self, term: &Term) -> DatValue {
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
        if self.current_file.is_none() {
            let spec = self.store.spec_by_export(name).unwrap();

            // generate initial values
            let file = self.store.file(&spec.filename).unwrap();
            let values: Vec<DatValue> = (0..file.rows_count)
                .map(|i| {
                    let kv_list: Vec<DatValue> = spec
                        .fields
                        .iter()
                        .map(move |field| {
                            let row_offset = file.rows_begin + i as usize * file.row_size;
                            DatValue::KeyValue(
                                field.name.clone(),
                                Box::new(file.read(row_offset, &field)),
                            )
                        })
                        .collect();
                    DatValue::Object(Box::new(DatValue::List(kv_list)))
                })
                .collect();

            self.current_field = None;
            self.current_file = Some(&spec.filename);
            self.identity = Some(DatValue::List(values));
        } else {
            self.enter_foreign();

            match self.identity.clone().unwrap_or(DatValue::Empty) {
                DatValue::Object(_) => {}
                DatValue::Iterator(_) => {} // TODO: clean up, and provide a helpful output message "did you mean to use []?"
                k => panic!(format!(
                    "Can't step into a field unless it's an object or iteratable. {:?}",
                    k
                )),
            };
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

            let value = self.identity.clone().unwrap_or(DatValue::Empty);
            let value = match value {
                DatValue::List(items) => DatValue::Iterator(items),
                _ => value,
            };

            let result = iterate(&value, |v| {
                let ids: Vec<u64> = match v {
                    DatValue::List(ids) => ids.clone(),     // TODO: yikes
                    DatValue::Iterator(ids) => ids.clone(), // TODO: yikes
                    DatValue::U64(id) => vec![DatValue::U64(*id)],
                    item => panic!(format!("Not a valid id for foreign key: {:?}", item)),
                }
                .iter()
                .filter_map(|v| match v {
                    DatValue::U64(i) => Some(*i),
                    DatValue::List(_) => None,
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

    fn to_iterable(&mut self) -> DatValue {
        self.enter_foreign();
        let value = self.identity.clone().unwrap_or(DatValue::Empty);
        let iteratable = match value {
            DatValue::List(list) => DatValue::Iterator(list),
            DatValue::Iterator(list) => DatValue::Iterator(list),
            DatValue::Object(content) => {
                let fields = match *content {
                    DatValue::List(fields) => fields,
                    _ => panic!("attempt to iterate an empty object"),
                };
                DatValue::Iterator(fields)
            }
            DatValue::Empty => DatValue::Iterator(Vec::with_capacity(0)),
            obj => panic!(format!(
                "unable to iterate, should i support this? {:?}",
                obj
            )),
        };
        iteratable
    }

    fn slice(&mut self, from: usize, to: usize) {
        let value = self.identity.clone().unwrap_or(DatValue::Empty);
        match value {
            DatValue::List(list) => {
                self.identity = Some(DatValue::List(
                    list[from..usize::min(to, list.len())].to_vec(),
                ))
            }
            DatValue::Str(str) => self.identity = Some(DatValue::Str(str[from..to].to_string())),
            _ => panic!("attempt to index non-indexable value {:?}", value),
        }
    }

    fn index(&mut self, index: usize) {
        let value = self.identity.clone().unwrap_or(DatValue::Empty);
        match value {
            DatValue::List(list) => match list.get(index) {
                Some(value) => self.identity = Some(value.clone()),
                None => panic!("attempt to index outside list"),
            },
            DatValue::Str(str) => match str.chars().nth(index) {
                Some(value) => self.identity = Some(DatValue::Str(value.to_string())),
                None => panic!("attempt to index outside string"),
            },
            _ => panic!("attempt to index non-indexable value {:?}", value),
        }
    }

    fn identity(&self) -> DatValue {
        self.identity.clone().unwrap_or(DatValue::Empty)
    }

    fn value(&mut self) -> DatValue {
        let current = self.current_file.unwrap();
        let spec = self.store.spec(current).unwrap();
        let file = self.store.file(current).unwrap();

        let identity = self.identity.clone().unwrap_or(DatValue::Empty);
        match identity {
            // TODO: extract to function
            DatValue::Object(entries) => {
                let v = match *entries {
                    DatValue::List(list) => {
                        let values: Vec<DatValue> = list
                            .iter()
                            .filter_map(|field| match field {
                                DatValue::KeyValue(key, value) => {
                                    if key == &self.current_field.clone().unwrap() {
                                        Some(*value.clone())
                                    } else {
                                        None
                                    }
                                }
                                _ => panic!("failed to extract value from kv"),
                            })
                            .collect();

                        values.first().unwrap_or(&DatValue::Empty).clone()
                    }
                    DatValue::KeyValue(key, value) => {
                        if key == self.current_field.clone().unwrap() {
                            *value.clone()
                        } else {
                            DatValue::Empty
                        }
                    }
                    _ => panic!(format!("failed to extract value from kv! {:?}", entries)),
                };
                return v.clone();
            }
            DatValue::Iterator(values) => {
                let result: Vec<DatValue> = values
                    .iter()
                    .map(|value| match value {
                        DatValue::KeyValue(k, v) => {
                            if self.current_field.clone().unwrap() == k.as_str() {
                                *v.clone()
                            } else {
                                DatValue::Empty
                            }
                        }
                        DatValue::Object(elements) => {
                            let obj = match *elements.clone() {
                                DatValue::List(fields) => fields,
                                _ => panic!(format!("uhm: {:?}", elements)),
                            };
                            obj.iter()
                                .filter_map(|field| match field {
                                    DatValue::KeyValue(k, v) => {
                                        if self.current_field.clone().unwrap() == k.as_str() {
                                            Some(*v.clone())
                                        } else {
                                            None
                                        }
                                    }
                                    asd => panic!(format!("what happened? {:?}", asd)),
                                })
                                .collect::<Vec<DatValue>>()
                                .first()
                                .unwrap_or(&DatValue::Empty)
                                .clone()
                        }
                        val => panic!(format!(
                            "Attempting to get field of non-iterable and non-object. {:?}",
                            val
                        )),
                    })
                    .collect();

                return DatValue::List(result);
            }
            DatValue::U64(i) => {
                // TODO: extract to function
                let kv_list: Vec<DatValue> = spec
                    .fields
                    .iter()
                    .map(move |field| {
                        let row_offset = file.rows_begin + i as usize * file.row_size;
                        DatValue::KeyValue(
                            field.name.clone(),
                            Box::new(file.read(row_offset, &field)),
                        )
                    })
                    .collect();

                return DatValue::Object(Box::new(DatValue::List(kv_list)));
            }
            _ => return DatValue::Empty,
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

    fn rows_from(&self, filepath: &str, indices: &[u64]) -> DatValue {
        let foreign_spec = self.store.spec(filepath).unwrap();
        let file = self.store.file(filepath).unwrap();

        let values: Vec<DatValue> = indices
            .iter()
            .map(|i| {
                let kv_list: Vec<DatValue> = foreign_spec
                    .fields
                    .iter()
                    .map(move |field| {
                        let row_offset =
                            file.rows_begin + usize::try_from(*i).unwrap() * file.row_size;
                        DatValue::KeyValue(
                            field.name.clone(),
                            Box::new(file.read(row_offset, &field)),
                        )
                    })
                    .collect();
                DatValue::Object(Box::new(DatValue::List(kv_list)))
            })
            .collect();

        if values.len() > 1 {
            DatValue::List(values)
        } else {
            values.first().unwrap_or(&DatValue::Empty).clone()
        }
    }
    // current value can be large datasets
    fn clone_with_value(&self, value: Option<DatValue>) -> TraversalContext {
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
fn iterate<F>(value: &DatValue, action: F) -> DatValue
where
    F: Fn(&DatValue) -> Option<DatValue>,
{
    match value {
        DatValue::Iterator(elements) => {
            DatValue::List(elements.iter().filter_map(|e| action(e)).collect())
        }
        _ => action(value).expect("non-iterable must return something"),
    }
}

fn reduce<F>(value: &DatValue, action: &mut F) -> DatValue
where
    F: FnMut(&DatValue) -> Option<DatValue>,
{
    let mut result = DatValue::Empty;
    match value {
        DatValue::Iterator(elements) => {
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
