use std::collections::HashMap;

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
    pub current_value: Option<Vec<DatValue>>,
}

pub trait DatNavigateImpl {
    fn child(&mut self, name: &str);
    fn index(&mut self, index: u64);
    fn value(&mut self) -> Vec<DatValue>;
    fn current_value(&self) -> Vec<DatValue>;
    fn clone(&self) -> DatNavigate;
    fn traverse_term(&mut self, term: &Term) -> Vec<DatValue>;
    fn traverse_terms(&mut self, parsed_terms: &Vec<Term>) -> Vec<DatValue>;
    fn traverse_terms_inner(&mut self, parsed_terms: &[Term]) -> Vec<DatValue>;
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
        } else {
            let current = self.current_file.unwrap();
            let spec = self.specs.get(current).unwrap();
            let field = spec.field(name);
            if field.is_some() && field.unwrap().is_foreign_key() {
                let field = field.unwrap();
                self.current_field = Some(name.to_string());
                self.current_value = Some(self.value());
                self.current_field = None;
                self.current_file = Some(&field.file);
                self.current_value = Some(self.value());
            } else {
                self.current_field = Some(name.to_string());
                self.current_value = Some(self.value());
            }
        }
    }

    fn index(&mut self, index: u64) {
        let asd = self.current_value.clone().unwrap_or(vec![]);
        match asd.get(index as usize) {
            Some(value) => self.current_value = Some(vec![value.clone()]),
            None => self.current_value = None,
        };
    }

    fn current_value(&self) -> Vec<DatValue> {
        self.current_value.clone().unwrap_or(Vec::with_capacity(0))
    }

    fn value(&mut self) -> Vec<DatValue> {
        let current = self.current_file.unwrap();
        let spec = self.specs.get(current).unwrap();
        let file = self.files.get(current).unwrap();

        let current_values = self.current_value.clone().unwrap_or(Vec::with_capacity(0));
        let current_type = current_values.first().unwrap_or(&DatValue::Empty);
        let data_type = match current_type {
            DatValue::Object(_) => "obj",
            DatValue::U32(_) => "number",
            DatValue::U64(_) => "number",
            _ => "",
        };

        if data_type == "obj" {
            let values: Vec<DatValue> = current_values
                .iter()
                .filter_map(|dat| match dat {
                    DatValue::Object(asd) => Some(asd),
                    _ => None,
                })
                .flat_map(|fields| {
                    fields.iter().filter_map(|field| match field {
                        DatValue::KeyValue(key, value) => {
                            if key == &self.current_field.clone().unwrap() {
                                Some(vec![*value.clone()])
                            } else {
                                None
                            }
                        },
                        DatValue::KeyList(key, value) => {
                            if key == &self.current_field.clone().unwrap() {
                                Some(value.to_vec())
                            } else {
                                None
                            }
                        }
                        _ => panic!("failed to extract value from kv"),
                    }).flatten()
                })
                .collect();
            return values;
        }
        if data_type == "number" {
            let values: Vec<DatValue> = current_values
                .iter()
                .filter_map(|value| match value {
                    DatValue::U32(i) => Some(*i as u64),
                    DatValue::U64(i) => Some(*i),
                    _ => None,
                })
                .map(|i| {
                    let kv_list = spec
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
                    DatValue::Object(kv_list)
                })
                .collect();
            return values;
        }

        if current_values.first().is_some() {
            match current_values.first().unwrap() {
                DatValue::Object(fields) => {
                    if self.current_field.is_some() {
                        let values: Vec<DatValue> = fields
                            .iter()
                            .filter_map(|field| match field {
                                DatValue::KeyValue(key, value) => {
                                    if key == &self.current_field.clone().unwrap().as_str() {
                                        Some(*value.clone())
                                    } else {
                                        None
                                    }
                                }
                                _ => None,
                            })
                            .collect();
                        self.current_value = Some(values);
                    }
                    self.current_value()
                }
                _ => {
                    let values: Vec<DatValue> = current_values
                        .iter()
                        .filter_map(|value| match value {
                            DatValue::U32(i) => Some(*i as u64),
                            DatValue::U64(i) => Some(*i),
                            _ => None,
                        })
                        .map(|i| {
                            let kv_list = spec
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
                            DatValue::Object(kv_list)
                        })
                        .collect();
                    self.current_value = Some(values);
                    self.current_value()
                }
            }
        } else if self.current_field.is_some() {
            let field = spec.field(self.current_field.clone().unwrap().as_str());
            (0..file.rows_count)
                .map(|i| {
                    let row_offset = file.rows_begin + i as usize * file.row_size;
                    file.read(row_offset, field.unwrap())
                })
                .collect()
        } else {
            (0..file.rows_count)
                .map(|i| {
                    let kv_list = spec
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
                    DatValue::Object(kv_list)
                })
                .collect()
        }
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

    fn traverse_terms(&mut self, parsed_terms: &Vec<Term>) -> Vec<DatValue> {
        let parts = parsed_terms.split(|term| match term {
            Term::comma => true,
            _ => false,
        });

        let asd: Vec<DatValue> = parts
            .flat_map(|terms| self.clone().traverse_terms_inner(&terms))
            .collect();
        self.current_value = Some(asd);
        self.current_value()
    }

    // Comma has be dealt with
    fn traverse_terms_inner(&mut self, terms: &[Term]) -> Vec<DatValue> {
        for term in terms {
            match term {
                Term::object(obj_terms) => {
                    let result = self.clone().traverse_terms(obj_terms);
                    self.current_value = Some(vec![DatValue::Object(result)]);
                }
                Term::kv(key, kv_terms) => {
                    let result = self.clone().traverse_terms(&kv_terms.to_vec());
                    self.current_value = Some(vec![DatValue::KeyList(key.to_string(), result)]);
                }
                _ => {
                    self.traverse_term(&term.clone());
                }
            }
        }

        self.current_value()
    }

    fn traverse_term(&mut self, term: &Term) -> Vec<DatValue> {
        match term {
            Term::by_name(key) => {
                self.child(key);
            }
            Term::by_index(i) => {
                self.index(*i);
            }
            _ => println!("unhandled term: {:?}", term),
        }
        self.current_value()
    }

}
