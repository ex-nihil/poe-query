use std::collections::HashMap;

use serde::Serialize;

use crate::dat::specification::FileSpec;
use crate::error::{closest_name, QueryError};

#[derive(Debug, Serialize)]
pub struct TableDescription {
    pub table: String,
    pub columns: Vec<ColumnDescription>,
}

#[derive(Debug, Serialize)]
pub struct ColumnDescription {
    pub name: String,
    #[serde(rename = "type")]
    pub column_type: String,
    pub references: Option<Reference>,
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct Reference {
    pub table: String,
    pub column: Option<String>,
}

pub fn table_names(specs: &HashMap<String, FileSpec>) -> Vec<String> {
    let mut names: Vec<String> = specs.values().map(|s| s.file_name.clone()).collect();
    names.sort();
    names
}

pub fn describe(specs: &HashMap<String, FileSpec>, table: &str) -> Result<TableDescription, QueryError> {
    let spec = specs.get(table)
        .or_else(|| specs.values().find(|s| s.file_name == table));
    let Some(spec) = spec else {
        let suggestion = closest_name(table, specs.values().map(|s| s.file_name.as_str()));
        return Err(QueryError::UnknownTable { name: table.to_string(), suggestion });
    };

    let columns = spec.file_fields.iter().map(|field| {
        ColumnDescription {
            name: field.field_name.clone(),
            column_type: field.field_type.clone(),
            references: field.file_name.as_ref().map(|target| Reference {
                table: target.clone(),
                column: field.file_reference_key.clone(),
            }),
            enum_values: field.enum_name.as_ref().map(|spec| spec.values().to_vec()),
        }
    }).collect();

    Ok(TableDescription { table: spec.file_name.clone(), columns })
}
