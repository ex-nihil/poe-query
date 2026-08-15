use std::collections::HashMap;

use serde::Serialize;

use crate::dat::specification::{FieldSpec, FileSpec};
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

/// One line per column: the column name mapped to a dense type notation.
/// Foreign keys show their target (`ModType`, `Tags.Id`), lists are
/// bracketed (`[u32]`, `[Tags]`), enums list their values.
pub fn describe_compact(specs: &HashMap<String, FileSpec>, table: &str) -> Result<serde_json::Value, QueryError> {
    let description = describe(specs, table)?;
    let spec = specs.values().find(|s| s.file_name == description.table).unwrap();

    let mut columns = serde_json::Map::new();
    for field in &spec.file_fields {
        columns.insert(field.field_name.clone(), serde_json::Value::String(compact_type(field)));
    }
    Ok(serde_json::json!({
        "table": description.table,
        "columns": columns,
    }))
}

fn compact_type(field: &FieldSpec) -> String {
    if let Some(enum_spec) = &field.enum_name {
        return format!("enum({})", enum_spec.values().join("|"));
    }
    let base = if let Some(target) = &field.file_name {
        match &field.file_reference_key {
            Some(column) => format!("{}.{}", target, column),
            None => target.clone(),
        }
    } else {
        // strip the internal storage encoding: "ref|string" -> "string"
        field.field_type.rsplit('|').next().unwrap_or(&field.field_type).to_string()
    };
    if field.field_type.starts_with("list|") {
        format!("[{}]", base)
    } else {
        base
    }
}
