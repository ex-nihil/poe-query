use std::collections::HashMap;
use std::convert::TryFrom;

use super::super::lang::Term;
use super::dat_file::DatFileRead;
use super::dat_file::{DatFile, DatValue};
use super::dat_spec::FieldSpecImpl;
use super::dat_spec::FileSpec;
use super::dat_spec::FileSpecImpl;

#[derive(Debug)]
pub struct DatNavigate<'a> {
    pub files: &'a HashMap<String, DatFile>,
    pub specs: &'a HashMap<String, FileSpec>,
    pub current_field: Option<String>,
    pub current_file: Option<&'a str>,
    pub current_value: Option<DatValue>,
}

pub trait DatNavigateImpl {
    fn child(&mut self, name: &str);
    fn index(&mut self, index: usize);
    fn slice(&mut self, from: usize, to: usize);
    fn iterate(&mut self);
    fn value(&mut self) -> DatValue;
    fn current_value(&self) -> DatValue;
    fn clone(&self) -> DatNavigate;
    fn traverse_term(&mut self, term: &Term) -> DatValue;
    fn traverse_terms(&mut self, parsed_terms: &Vec<Term>) -> DatValue;
    fn traverse_terms_inner(&mut self, parsed_terms: &[Term]) -> DatValue;
}

impl DatNavigateImpl for DatNavigate<'_> {
    fn child(&mut self, name: &str) {
        if self.current_file.is_none() {
            let spec = self
                .specs
                .values()
                .find(|&s| s.export == name)
                .expect("no spec with export");
            self.current_field = None;
            self.current_file = Some(&spec.filename);
            self.current_value = Some(self.value());

            // generate initial values
            let current = self.current_file.unwrap();
            let file = self.files.get(current).unwrap();
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
            if values.len() > 1 {
                self.current_value = Some(DatValue::List(values));
            } else {
                self.current_value = Some(values.first().unwrap_or(&DatValue::Empty).clone());
            }
        } else {
            let current = self.current_file.unwrap();
            let spec = self.specs.get(current).unwrap();
            let field = spec.field(name);
            if field.is_some() && field.unwrap().is_foreign_key() {
                let field = field.unwrap();
                self.current_field = Some(name.to_string());
                self.current_value = Some(self.value());
                self.current_field = None;

                let ids: Vec<u64> = match self.current_value.clone().unwrap_or(DatValue::Empty) {
                    DatValue::List(ids) => ids,
                    DatValue::U64(id) => vec![DatValue::U64(id)],
                    DatValue::U32(id) => vec![DatValue::U32(id)],
                    item => panic!(format!("Not a valid id for foreign key: {:?}", item)),
                }
                .iter()
                .map(|v| match v {
                    DatValue::U32(i) => *i as u64,
                    DatValue::U64(i) => *i,
                    _ => panic!(format!("value {:?}", v)),
                })
                .collect();

                let spec = self
                    .specs
                    .values()
                    .find(|&s| s.filename == field.file)
                    .expect(format!("No spec for export: {}", name).as_str());
                let file = self.files.get(&field.file).unwrap();
                let values: Vec<DatValue> = ids
                    .iter()
                    .map(|i| {
                        let kv_list: Vec<DatValue> = spec
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
                self.current_field = None;
                self.current_file = Some(spec.filename.as_str());
                self.current_value = Some(DatValue::List(values));
            } else {
                match self.current_value.clone().unwrap_or(DatValue::Empty) {
                    DatValue::Object(_) => {}
                    DatValue::Iterator(_) => {} // TODO: clean up, and provide a helpful output message "did you mean to use []?"
                    k => panic!(format!(
                        "Can't step into a field unless it's an object or iteratable. {:?}",
                        k
                    )),
                };
                self.current_field = Some(name.to_string());
                self.current_value = Some(self.value());
                self.current_field = None;
            }
        }
    }

    fn iterate(&mut self) {
        let value = self.current_value.clone().unwrap_or(DatValue::Empty);
        let iteratable = match value {
            DatValue::List(list) => DatValue::Iterator(list),
            DatValue::Object(content) => {
                let fields = match *content {
                    DatValue::List(fields) => fields,
                    _ => panic!("attempt to iterate an empty object"),
                };
                DatValue::Iterator(fields)
            }
            _ => panic!("unable to iterate, should i support this?"),
        };
        self.current_value = Some(iteratable);
    }

    fn slice(&mut self, from: usize, to: usize) {
        let value = self.current_value.clone().unwrap_or(DatValue::Empty);
        match value {
            DatValue::List(list) => {
                self.current_value = Some(DatValue::List(
                    list[from..usize::min(to, list.len())].to_vec(),
                ))
            }
            DatValue::Str(str) => {
                self.current_value = Some(DatValue::Str(str[from..to].to_string()))
            }
            _ => panic!("attempt to index non-indexable value {:?}", value),
        }
    }

    fn index(&mut self, index: usize) {
        let value = self.current_value.clone().unwrap_or(DatValue::Empty);
        match value {
            DatValue::List(list) => match list.get(index) {
                Some(value) => self.current_value = Some(value.clone()),
                None => panic!("attempt to index outside list"),
            },
            DatValue::Str(str) => match str.chars().nth(index) {
                Some(value) => self.current_value = Some(DatValue::Str(value.to_string())),
                None => panic!("attempt to index outside string"),
            },
            _ => panic!("attempt to index non-indexable value {:?}", value),
        }
    }

    fn current_value(&self) -> DatValue {
        self.current_value.clone().unwrap_or(DatValue::Empty)
    }

    fn value(&mut self) -> DatValue {
        let current = self.current_file.unwrap();
        let spec = self.specs.get(current).unwrap();
        let file = self.files.get(current).unwrap();

        let current_value = self.current_value.clone().unwrap_or(DatValue::Empty);
        match current_value {
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

                        if values.len() > 1 {
                            DatValue::List(values)
                        } else {
                            values.first().unwrap_or(&DatValue::Empty).clone()
                        }
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
                                    _ => None,
                                })
                                .collect::<Vec<DatValue>>()
                                .first()
                                .unwrap_or(&DatValue::Empty)
                                .clone()
                        }
                        _ => panic!("Attempting to get field of non-iterable and non-object"),
                    })
                    .collect();
                return if result.len() > 1 {
                    DatValue::List(result)
                } else {
                    result.first().unwrap().clone()
                };
            }
            DatValue::U32(i) => {
                println!("calculating value U32");
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
            DatValue::U64(i) => {
                println!("calculating value U64");
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
            _ => return DatValue::Empty, // ???
        };
    }

    fn clone(&self) -> DatNavigate {
        DatNavigate {
            files: self.files,
            specs: self.specs,
            current_field: self.current_field.clone(),
            current_file: self.current_file,
            current_value: self.current_value.clone(),
        }
    }

    fn traverse_terms(&mut self, parsed_terms: &Vec<Term>) -> DatValue {
        let parts = parsed_terms.split(|term| match term {
            Term::comma => true,
            _ => false,
        });

        let values: Vec<DatValue> = parts
            .map(|terms| self.clone().traverse_terms_inner(&terms))
            .collect();
        let value = if values.len() > 1 {
            DatValue::List(values)
        } else {
            values.first().unwrap().clone()
        };

        self.current_value = Some(value);
        self.current_value()
    }

    // Comma has be dealt with
    fn traverse_terms_inner(&mut self, terms: &[Term]) -> DatValue {
        for term in terms {
            match term {
                Term::iterator => self.iterate(),
                Term::object(obj_terms) => {
                    if let Some(value) = &self.current_value {
                        match value {
                            DatValue::Iterator(items) => {
                                let result = items
                                    .iter()
                                    .map(|item| {
                                        let mut clone = self.clone();
                                        clone.current_value =
                                            Some(DatValue::Iterator(vec![item.clone()]));
                                        let output = clone.traverse_terms(obj_terms);
                                        DatValue::Object(Box::new(DatValue::List(vec![output])))
                                    })
                                    .collect();

                                self.current_value = Some(DatValue::List(result));
                            }
                            DatValue::Object(_) => {
                                let mut clone = self.clone();
                                let output = clone.traverse_terms(obj_terms);
                                self.current_value = Some(DatValue::Object(Box::new(output)));
                            }
                            item => {
                                panic!(format!("not implemented - object creation on: {:?}", item))
                            }
                        };
                    }
                    // TODO: support object creation without input
                }
                Term::kv(key, kv_terms) => {
                    let result = self.clone().traverse_terms(&kv_terms.to_vec());
                    self.current_value =
                        Some(DatValue::KeyValue(key.to_string(), Box::new(result)));
                }
                _ => {
                    self.traverse_term(&term.clone());
                }
            }
        }

        self.current_value()
    }

    fn traverse_term(&mut self, term: &Term) -> DatValue {
        match term {
            Term::by_name(key) => {
                self.child(key);
            }
            Term::by_index(i) => {
                self.index(*i);
            }
            Term::slice(from, to) => {
                self.slice(*from, *to);
            }
            _ => println!("unhandled term: {:?}", term),
        }
        self.current_value()
    }
}
