use super::util;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, PartialEq, Deserialize, Clone)]
pub struct FileSpec {
    pub filename: String,
    pub fields: Vec<FieldSpec>,
    pub export: String,
}

#[derive(Debug, PartialEq, Deserialize, Clone)]
pub struct FieldSpec {
    #[serde(default = "undefined")]
    pub name: String,

    #[serde(alias = "type")]
    pub datatype: String,

    #[serde(default = "empty")]
    pub file: String,

    #[serde(skip)]
    pub offset: u64,
}

fn undefined() -> String {
    "undefined".to_string()
}

fn empty() -> String {
    "".to_string()
}

impl FileSpec {
    pub fn read(filename: &Path) -> FileSpec {
        let reader = util::read_from_path(filename);
        let spec: FileSpec =
            serde_yaml::from_reader(reader).expect("Unable to parse specification");

        return FileSpec {
            fields: update_with_offsets(spec.fields),
            ..spec
        };
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
