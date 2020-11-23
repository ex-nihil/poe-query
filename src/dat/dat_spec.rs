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
    pub name: String,
    pub rowid: u64,
    pub r#type: String,
    pub file: String,

    #[serde(skip)]
    pub offset: u64,
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

    pub fn is_foreign_key(&self, field: &FieldSpec) -> bool {
        !field.file.is_empty() && field.file != "~"
    }
}

fn update_with_offsets(fields: Vec<FieldSpec>) -> Vec<FieldSpec> {
    let mut offset = 0;
    fields
        .iter()
        .map(|field| {
            let datatype = field.r#type.split("|").next().unwrap();
            let size = match datatype {
                "u64" | "list" => 8,
                _ => 4,
            };
            let updated = FieldSpec {
                offset,
                name: field.name.clone(),
                rowid: field.rowid,
                r#type: field.r#type.clone(),
                file: field.file.clone(),
            };
            offset += size;
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
