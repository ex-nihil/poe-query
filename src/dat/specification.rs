use std::fmt;
use std::collections::HashMap;
use std::fmt::Formatter;
use std::path::Path;

use apollo_parser::ast::AstNode;
use serde::Deserialize;

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
    pub enum_name: Option<EnumSpec>,
    pub field_offset: usize,
}

impl fmt::Display for FieldSpec {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", self.field_name, self.field_type)
    }
}

impl FileSpec {
    pub fn read_enum_specs(path: &Path) -> HashMap<String, EnumSpec> {
        use apollo_parser::Parser;
        use apollo_parser::ast::Definition;

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

    pub fn read_file_specs(path: &Path, enum_specs: &HashMap<String, EnumSpec>) -> HashMap<String, FileSpec> {
        use apollo_parser::Parser;
        use apollo_parser::ast::Definition;
        use apollo_parser::ast::Type;

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
                    Definition::ObjectTypeDefinition(obj) => {
                        let filename = obj.name().unwrap().text().to_string();
                        let mut offset = 0;

                        let mut fields = Vec::new();
                        for field in obj.fields_definition().unwrap().field_definitions() {
                            let current_offset = offset;
                            let name = field.name().unwrap().text();

                            let mut is_path_field = false;
                            if let Some(field_directives) = field.directives().map(|x| x.directives()) {
                                for directive in field_directives {
                                    if directive.name().unwrap().text().as_str() == "file" {
                                        is_path_field = true;
                                    }
                                }
                            }

                            let mut is_list = false;

                            let type_name = match field.ty().unwrap() {
                                Type::NamedType(it) => {
                                    let spec_type = it.syntax().text().to_string();
                                    match spec_type.as_str() {
                                        "rid" | "i64" | "u64" => offset += 8,
                                        "u32" | "i32" | "f32" | "string" => offset += 4,
                                        "bool" | "u8" => offset += 1,
                                        t if t == filename => offset += 4, // self reference
                                        _ => offset += 8,
                                    }
                                    spec_type
                                }
                                Type::ListType(it) => {
                                    offset += 8;
                                    is_list = true;
                                    it.syntax().first_child().unwrap().text().to_string()
                                }
                                node => unimplemented!("Unhandled node: {:?}", node),
                            };

                            let type_name = match type_name.as_str() {
                                "rid" => "u64".to_string(),
                                _ if is_path_field => "path".to_string(),
                                _ => type_name
                            };

                            let enum_spec = enum_specs.get(type_name.as_str());

                            let key_file = match type_name.as_str() {
                                "i32" | "bool" | "string" | "f32" | "u32" | "path" | "_" => None,
                                _ if enum_spec.is_some() => None,
                                fk => Some(fk.to_string())
                            };

                            let mut type_value = match (is_list, &key_file) {
                                (true, Some(_)) => {
                                    "list|u32".to_string()
                                }
                                (true, None) => {
                                    "list|".to_owned() + &type_name
                                }
                                (false, Some(_)) => "u32".to_string(),
                                (false, None) => match type_name.as_str() {
                                    "string" => "ref|string",
                                    "path" => "ref|path",
                                    t => t
                                }.to_string(),
                            };
                            if enum_spec.is_some() {
                                offset -= 4;
                                type_value = "u32".to_string();
                            }

                            fields.push(FieldSpec {
                                field_name: name.to_string(),
                                field_type: type_value.to_string(),
                                file_name: key_file,
                                enum_name: enum_spec.cloned(),
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
            }).collect()
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
