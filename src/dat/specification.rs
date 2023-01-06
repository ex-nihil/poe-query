use std::fmt;
use std::collections::HashMap;
use std::fmt::Formatter;
use std::path::Path;

use apollo_parser::ast::AstNode;
use serde::Deserialize;
use apollo_parser::Parser;
use apollo_parser::ast::Definition;
use apollo_parser::ast::Type;

#[derive(Debug, PartialEq, Eq, Deserialize, Clone)]
pub struct FileSpec {
    pub file_name: String,
    pub file_fields: Vec<FieldSpec>,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Clone)]
pub struct EnumSpec {
    enum_name: String,
    first_index: usize,
    enum_values: Vec<String>,
}

impl EnumSpec {
    pub fn value(&self, index: usize) -> String {
        self.enum_values.get(index - self.first_index)
            .unwrap_or(&"".to_string()).to_string()
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize, Clone)]
pub struct FieldSpec {
    pub field_name: String,
    pub field_type: String,
    pub file_name: Option<String>,
    pub file_reference_key: Option<String>,
    pub enum_name: Option<EnumSpec>,
    pub field_size: usize,
    pub field_offset: usize,
}

impl fmt::Display for FileSpec {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", self.file_name)?;
        self.file_fields.iter().for_each(|field| {
            writeln!(f, "\t{:03} {} enum({}) ref({}) fk({}) ", field.field_offset, field, field.enum_name.is_some(), field.file_reference_key.is_some(), field.file_name.is_some()).unwrap();
        });
        Ok(())
    }
}

impl fmt::Display for FieldSpec {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", self.field_name, self.field_type)
    }
}

impl FileSpec {
    pub fn read_enum_specs(path: &Path) -> HashMap<String, EnumSpec> {

        std::fs::read_dir(path).expect("spec path does not exist")
            .filter_map(|directory| directory.ok().map(|entry| entry.path()))
            .filter(|file_path| file_path.is_file() && file_path.extension().expect("gql file not found").to_string_lossy() == "gql")
            .flat_map(|file_path| {
                let text = std::fs::read_to_string(file_path).unwrap();

                let parser = Parser::new(&text);
                let ast = parser.parse();

                assert_eq!(ast.errors().len(), 0);
                ast.document().definitions()
            })
            .filter_map(|definition| {
                match definition {
                    Definition::ObjectTypeDefinition(_) => None,
                    Definition::EnumTypeDefinition(obj) => {
                        let enum_name = obj.name().unwrap().text();

                        let mut index = 0;
                        for directive in obj.directives().unwrap().directives() {
                            if directive.name().unwrap().text().as_str() == "indexing" {
                                let first = directive.arguments().unwrap().arguments().find(|x| x.name().unwrap().text() == "first").unwrap().value().unwrap().syntax().text().to_string();
                                index = first.parse::<usize>().unwrap();
                            }
                        }
                        let mut values = Vec::new();
                        for field in obj.enum_values_definition().unwrap().enum_value_definitions() {
                            let value = field.enum_value().unwrap().name().unwrap().text();
                            values.push(value.to_string())
                        }

                        Some((
                            enum_name.to_string(),
                            EnumSpec {
                                enum_name: enum_name.to_string(),
                                first_index: index,
                                enum_values: values,
                            }
                        ))
                    }
                    def => unimplemented!("Unhandled definition: {:?}", def),
                }
            }).collect::<HashMap<_, _>>()
    }

    pub fn read_specs_transform_definitions<F, T>(path: &Path, transform: F) -> HashMap<String, T>
        where
            F: Fn(Definition) -> Option<(String, T)>,
    {
        let specs: HashMap<_, T> = std::fs::read_dir(path).expect("spec path does not exist")
            .filter_map(|directory| directory.ok().map(|entry| entry.path()))
            .filter(|file_path| file_path.is_file() && file_path.extension().expect("gql file not found").to_string_lossy() == "gql")
            .flat_map(|file_path| {
                let text = std::fs::read_to_string(file_path).unwrap();

                let parser = Parser::new(&text);
                let ast = parser.parse();

                assert_eq!(ast.errors().len(), 0);
                ast.document().definitions()
            })
            .filter_map(transform)
            .collect();
        specs
    }



    pub fn read_file_specs(path: &Path, enum_specs: &HashMap<String, EnumSpec>, file_specs: &HashMap<String, FileSpec>) -> HashMap<String, FileSpec> {
        Self::read_specs_transform_definitions(path, |definition| {
            match definition {
                Definition::ObjectTypeDefinition(obj) => {
                    let filename = obj.name().unwrap().text().to_string();
                    let mut offset = 0;

                    let mut fields = Vec::new();
                    for field in obj.fields_definition().unwrap().field_definitions() {
                        let current_offset = offset;
                        let name = field.name().unwrap().text();

                        let mut is_path_field = false;
                        let mut reference_key = None;
                        if let Some(field_directives) = field.directives().map(|x| x.directives()) {
                            for directive in field_directives {
                                // @file(ext: ".dds")
                                if directive.name().unwrap().text().as_str() == "file" {
                                    is_path_field = true;
                                }
                                // @ref(column: "Id")
                                if directive.name().unwrap().text().as_str() == "ref" {
                                    let first = directive.arguments().unwrap().arguments().find(|x| x.name().unwrap().text() == "column").unwrap().value().unwrap().syntax().text().to_string();
                                    reference_key = Some(first.replace("\"",""));
                                }
                            }
                        }

                        let mut is_list = false;

                        let field_type = field.ty().unwrap();

                        let type_name = match &field_type {
                            Type::NamedType(it) => {
                                it.syntax().text().to_string()
                            }
                            Type::ListType(it) => {
                                is_list = true;
                                it.syntax().first_child().unwrap().text().to_string()
                            }
                            node => unimplemented!("Unhandled node: {:?}", node),
                        };

                        let enum_spec = enum_specs.get(type_name.as_str());

                        let mut type_name = match type_name.as_str() {
                            "rid" => "u64".to_string(),
                            _ if is_path_field => "path".to_string(),
                            _ => type_name
                        };

                        let key_file = match type_name.as_str() {
                            "i32" | "bool" | "string" | "f32" | "u32" | "path" | "_" => None,
                            _ if enum_spec.is_some() => None,
                            fk => Some(fk.to_string())
                        };

                        if reference_key.is_some() && key_file.is_some() {
                            match file_specs.get(key_file.as_ref().unwrap()) {
                                None => {},
                                Some(file_spec) => {
                                    type_name = file_spec.file_fields.iter()
                                        .find(|x| Some(&x.field_name) == reference_key.as_ref())
                                        .map(|field| field.field_type.clone())
                                        .unwrap();
                                }
                            }
                        }

                        let mut field_size: usize = match type_name.as_str() {
                            "bool" | "u8" => 1,
                            "u32" | "i32" | "f32" => 4,
                            "i64" | "u64" | "string" => 8,
                            _ if reference_key.is_some() && key_file.is_some() => {
                                match file_specs.get(key_file.as_ref().unwrap()) {
                                    None => 16,
                                    Some(file_spec) => {
                                        file_spec.file_fields.iter()
                                            .find(|x| Some(&x.field_name) == reference_key.as_ref())
                                            .map(|field| field.field_offset)
                                            .unwrap_or_else(|| 16)
                                    }
                                }
                            },
                            _ if enum_spec.is_some() => 4,
                            t if t == filename => 8, // self reference
                            _ => 16,
                        };

                        if is_list {
                            field_size = 16;
                        }

                        offset += field_size;


                        let mut type_value = match (is_list, &key_file) {
                            (false, Some(_)) => "u64".to_string(),
                            (true, Some(_)) => "list|u64".to_string(),
                            (true, None) => {
                                "list|".to_owned() + &type_name
                            }
                            (false, None) => match type_name.as_str() {
                                "string" => "ref|string",
                                "path" => "ref|path",
                                t => t
                            }.to_string(),
                        };
                        if enum_spec.is_some() {
                            type_value = "u32".to_string();
                        }

                        fields.push(FieldSpec {
                            field_name: name.to_string(),
                            field_type: type_value.to_string(),
                            file_name: key_file,
                            file_reference_key: reference_key,
                            enum_name: enum_spec.cloned(),
                            field_size,
                            field_offset: current_offset,
                        });
                    }

                    let spec = FileSpec {
                        file_name: filename,
                        file_fields: fields,
                    };

                    Some((
                        spec.file_name.clone(),
                        spec
                    ))
                }
                Definition::EnumTypeDefinition(_) => None,
                def => unimplemented!("Unhandled definition: {:?}", def),
            }
        })
    }

    pub fn field_size(field: &FieldSpec) -> usize {
        let datatype = field.field_type.split('|').next().unwrap();
        match datatype {
            "u64" | "i64" | "list" => 8,
            "bool" | "u8" => 1,
            _ => 4,
        }
    }
}

pub trait FileSpecImpl {
    fn field(&self, key: &str) -> Option<&FieldSpec>;
}

pub trait FieldSpecImpl {
    fn is_foreign_key(&self) -> bool;
}

impl FieldSpecImpl for FieldSpec {
    fn is_foreign_key(&self) -> bool {
        self.file_name.is_some()
    }
}

impl FileSpecImpl for FileSpec {
    fn field(&self, key: &str) -> Option<&FieldSpec> {
        self.file_fields.iter().find(|&f| f.field_name == key)
    }
}
