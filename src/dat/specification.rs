use std::collections::HashMap;
use super::util;
use serde::Deserialize;
use std::path::Path;
use apollo_parser::ast::AstNode;
use log::{error, warn};

#[derive(Debug, PartialEq, Deserialize, Clone)]
pub struct FileSpec {
    pub filename: String,
    pub fields: Vec<FieldSpec>,
    pub export: String,
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
pub struct FieldSpec {
    pub name: String,
    pub datatype: String,
    pub file: String,
    pub offset: u64,
}

fn undefined() -> String {
    "undefined".to_string()
}

fn empty() -> String {
    "".to_string()
}

impl FileSpec {

    pub fn read_all_specs(path: &str) -> HashMap<String, FileSpec> {

        let whitelist = vec![
        ];

        let blacklist = vec![
            "ShopTag",
            "ItemVisualEffect",
            "MiscBeams",
            "HarvestCraftOptions",
            "CurrencyItems",
            "BestiaryCapturableMonsters",
            "HellscapeImmuneMonsters",
            "TriggerSpawners",
            "BuffVisualOrbs",
            "ModFamily",
            "Chests",
            "BetrayalDialogue",
            "ItemVisualIdentity",
            "MapConnections",
            "CharacterStartStates"
        ];
        use apollo_parser::Parser;
        use apollo_parser::ast::Definition;
        use apollo_parser::ast::Type;

        let mut file_specs: HashMap<String, FileSpec> = HashMap::new();

        let paths = std::fs::read_dir(path).expect("spec path does not exist");
        let mut asd: Vec<_> = paths
            .filter_map(Result::ok)
            .map(|d| d.path())
            .filter(|pb| pb.is_file() && pb.extension().expect("gql file not found").to_string_lossy() == "gql")
            //.map(|p| p.as_path())
            .collect();

        asd.sort(); // TODO: RIP core is last if sorted alphabetically

        asd.iter().for_each(|pb| {
            //println!("Path: {:?}", pb);
            let text = std::fs::read_to_string(pb).unwrap();

            let parser = Parser::new(&text);
            let ast = parser.parse();


            assert_eq!(ast.errors().len(), 0);
            for def in ast.document().definitions() {
                match def {
                    Definition::ObjectTypeDefinition(obj) => {
                        let filename = obj.name().unwrap().text().to_string();
                        if !whitelist.is_empty() && !whitelist.contains(&filename.as_str()) {
                            continue;
                        }
                        if !blacklist.is_empty() && blacklist.contains(&filename.as_str()) {
                            continue;
                        }
                        let mut offset = 0;

                        let mut fields = Vec::new();
                        for field in obj.fields_definition().unwrap().field_definitions() {
                            let current_offset = offset;
                            let name = field.name().unwrap().text();
                            let type_syntax = field.ty().unwrap().syntax().clone();

                            let mut is_list = false;
                            let mut asd = match field.ty().unwrap() {
                                Type::NamedType(it) => {
                                    match it.syntax().text().to_string().as_str() {
                                        "i64" | "u64" => offset += 8,
                                        "bool" | "u8" => offset += 1,
                                        _ => offset += 4
                                    }
                                    it.syntax().text().to_string()
                                },
                                Type::ListType(it) => {
                                    offset += 8;
                                    is_list = true;
                                    it.syntax().first_child().unwrap().text().to_string()
                                },
                                node => unimplemented!("Unhandled node: {:?}", node),
                            };


                            let key_file = match asd.as_str() {
                                "i32" | "bool" | "rid" | "string" | "f32" => None,
                                t => {
                                    Some(t.to_string())
                                }
                            };
/*
                            match key_file {
                                None => {}
                                Some(x) => error!("type: {}", x)
                            }
*/
                            let type_value = match (is_list, &key_file) {
                                (true, Some(file)) => {
                                    "list|ptr".to_string()
                                }
                                (true, None) => {
                                    "list|".to_owned() + &asd
                                }
                                (_, Some(_)) => "ptr".to_string(),
                                (_, None) => match asd.as_str() {
                                    "string" => "ref|string",
                                    "rid" => "u32",
                                    t => t
                                }.to_string(),
                            };

/*
                            let kind = type_syntax.kind();

                            let mut converted_type = match asd.as_str() {
                                "string" => "ref|string",
                                "rid" => "u32",
                                t => t
                            };


                            if key_file.is_some() {
                                converted_type = "ptr";
                            }

 */
                            fields.push(FieldSpec {
                                name: name.to_string(),
                                datatype: type_value.to_string(),
                                file: key_file.map(|file| format!("Data/{}.dat", file)).unwrap_or("".to_string()) ,
                                offset: current_offset
                            });
                        }

                        for field in &fields {
                            warn!("{:?}", field);
                        }
                        file_specs.insert(format!("Data/{}.dat", filename), FileSpec {
                            filename: format!("Data/{}.dat", filename),
                            fields,
                            export: filename
                        });
                    }
                    Definition::EnumTypeDefinition(obj) => {
                        //println!("enum {}", obj.name().unwrap().text());
                        for field in obj.enum_values_definition().unwrap().enum_value_definitions() {
                            //println!("\t{}", field.enum_value().unwrap().name().unwrap().text());
                        }
                    }
                    def => unimplemented!("Unhandled definition: {:?}", def),
                }
            }
        });
        return file_specs;
    }

    pub fn field_size(field: &FieldSpec) -> u64 {
        let datatype = field.datatype.split("|").next().unwrap();
        match datatype {
            "u64" | "list" => 8,
            "bool" | "u8" => 1,
            _ => 4,
        }
    }
}

fn update_with_offsets(fields: Vec<FieldSpec>) -> Vec<FieldSpec> {
    let mut offset = 0;
    fields
        .iter()
        .map(|field| {
            let updated = FieldSpec {
                offset,
                name: field.name.clone(),
                datatype: field.datatype.clone(),
                file: field.file.clone(),
            };
            offset += FileSpec::field_size(field);
            return updated;
        })
        .collect()
}

pub trait FileSpecImpl {
    fn field(&self, key: &str) -> Option<&FieldSpec>;
}

pub trait FieldSpecImpl {
    fn is_foreign_key(&self) -> bool;
}

impl FieldSpecImpl for FieldSpec {
    fn is_foreign_key(&self) -> bool {
        !self.file.is_empty() && self.file != "~"
    }
}

impl FileSpecImpl for FileSpec {
    fn field(&self, key: &str) -> Option<&FieldSpec> {
        self.fields.iter().find(|&f| f.name == key)
    }
}
