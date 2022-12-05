use std::fmt;
use std::collections::HashMap;
use std::fmt::Formatter;
use std::path::Path;

use apollo_parser::ast::AstNode;
use serde::Deserialize;

#[derive(Debug, PartialEq, Eq, Deserialize, Clone)]
pub struct FileSpec {
    pub filename: String,
    pub fields: Vec<FieldSpec>
}

#[derive(Debug, PartialEq, Eq, Deserialize, Clone)]
pub struct EnumSpec {
    name: String,
    first_index: usize,
    values: Vec<String>,
}

impl EnumSpec {
    pub fn value(&self, index: usize) -> String {
        self.values.get(index - self.first_index).unwrap_or(&"".to_string()).to_string()
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize, Clone)]
pub struct FieldSpec {
    pub name: String,
    pub datatype: String,
    pub file: Option<String>,
    pub enum_name: Option<EnumSpec>,
    pub offset: u64,
}

impl fmt::Display for FieldSpec {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", self.name, self.datatype)
    }
}

impl FileSpec {

    pub fn read_all_enum_specs(path: &Path) -> HashMap<String, EnumSpec> {
        use apollo_parser::Parser;
        use apollo_parser::ast::Definition;

        let mut enum_specs: HashMap<_, _> = HashMap::new();

        let paths = std::fs::read_dir(path).expect("spec path does not exist");
        paths
            .filter_map(Result::ok)
            .map(|d| d.path())
            .filter(|pb| pb.is_file() && pb.extension().expect("gql file not found").to_string_lossy() == "gql")
            .for_each(|pb| {
            let text = std::fs::read_to_string(pb).unwrap();

            let parser = Parser::new(&text);
            let ast = parser.parse();

            assert_eq!(ast.errors().len(), 0);
            for def in ast.document().definitions() {
                match def {
                    Definition::ObjectTypeDefinition(_) => {}
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

                        enum_specs.insert(enum_name.to_string(), EnumSpec {
                            name: enum_name.to_string(),
                            first_index: index,
                            values
                        });

                    }
                    def => unimplemented!("Unhandled definition: {:?}", def),
                }
            }
        });
        enum_specs
    }

    pub fn read_all_specs(path: &Path, enum_specs: &HashMap<String, EnumSpec>) -> HashMap<String, FileSpec> {
        use apollo_parser::Parser;
        use apollo_parser::ast::Definition;
        use apollo_parser::ast::Type;

        let mut file_specs: HashMap<String, FileSpec> = HashMap::new();

        let paths = std::fs::read_dir(path).expect("spec path does not exist");
        paths
            .filter_map(Result::ok)
            .map(|d| d.path())
            .filter(|pb| pb.is_file() && pb.extension().expect("gql file not found").to_string_lossy() == "gql")
            .for_each(|pb| {
            let text = std::fs::read_to_string(pb).unwrap();

            let parser = Parser::new(&text);
            let ast = parser.parse();

            assert_eq!(ast.errors().len(), 0);
            for def in ast.document().definitions() {
                match def {
                    Definition::ObjectTypeDefinition(obj) => {

                        let filename = obj.name().unwrap().text().to_string();
                        let mut offset = 0;

                        let mut fields = Vec::new();
                        for field in obj.fields_definition().unwrap().field_definitions() {
                            let current_offset = offset;
                            let name = field.name().unwrap().text();

                            let mut is_path_field = false;
                            if let Some(field_directives)  = field.directives().map(|x| x.directives()) {
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
                                },
                                Type::ListType(it) => {
                                    offset += 8;
                                    is_list = true;
                                    it.syntax().first_child().unwrap().text().to_string()
                                },
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
                                name: name.to_string(),
                                datatype: type_value.to_string(),
                                file: key_file,
                                enum_name: enum_spec.cloned(),
                                offset: current_offset
                            });
                        }

                        let spec = FileSpec {
                            filename,
                            fields
                        };
                        file_specs.insert(spec.filename.clone(), spec);
                    }
                    Definition::EnumTypeDefinition(_) => {}
                    def => unimplemented!("Unhandled definition: {:?}", def),
                }
            }
        });
        file_specs
    }

    pub fn field_size(field: &FieldSpec) -> u64 {
        let datatype = field.datatype.split('|').next().unwrap();
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
        self.file.is_some()
    }
}

impl FileSpecImpl for FileSpec {
    fn field(&self, key: &str) -> Option<&FieldSpec> {
        self.fields.iter().find(|&f| f.name == key)
    }
}
