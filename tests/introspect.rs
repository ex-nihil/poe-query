mod common;

use std::collections::HashMap;

use common::store::{field, foreign_key, spec};
use poe_query_lib::dat::specification::FileSpec;
use poe_query_lib::error::QueryError;
use poe_query_lib::introspect;

fn example_specs() -> HashMap<String, FileSpec> {
    let mods = spec("Mods", vec![
        field("Id", "ref|string", 8, 0),
        field("Weight", "u32", 4, 8),
        foreign_key("ModTypeKey", "ModType", 12),
    ]);
    let modtype = spec("ModType", vec![
        field("Name", "ref|string", 8, 0),
    ]);
    HashMap::from([
        ("Mods".to_string(), mods),
        ("ModType".to_string(), modtype),
    ])
}

#[test]
fn lists_tables_sorted() {
    assert_eq!(introspect::table_names(&example_specs()), vec!["ModType", "Mods"]);
}

#[test]
fn describes_columns_and_references() {
    let description = introspect::describe(&example_specs(), "Mods").unwrap();
    let json = serde_json::to_value(&description).unwrap();
    assert_eq!(json["table"], "Mods");
    assert_eq!(json["columns"][0]["name"], "Id");
    assert_eq!(json["columns"][0]["type"], "ref|string");
    assert_eq!(json["columns"][0]["references"], serde_json::Value::Null);
    assert_eq!(json["columns"][2]["name"], "ModTypeKey");
    assert_eq!(json["columns"][2]["references"]["table"], "ModType");
}

#[test]
fn compact_describe_maps_columns_to_dense_types() {
    let compact = introspect::describe_compact(&example_specs(), "Mods").unwrap();
    assert_eq!(compact["table"], "Mods");
    assert_eq!(compact["columns"]["Id"], "string");
    assert_eq!(compact["columns"]["Weight"], "u32");
    assert_eq!(compact["columns"]["ModTypeKey"], "ModType");
    // schema order is preserved, not alphabetical
    let keys: Vec<&String> = compact["columns"].as_object().unwrap().keys().collect();
    assert_eq!(keys, ["Id", "Weight", "ModTypeKey"]);
}

#[test]
fn describe_unknown_table_suggests() {
    let error = introspect::describe(&example_specs(), "Modz").unwrap_err();
    assert_eq!(error, QueryError::UnknownTable {
        name: "Modz".to_string(),
        suggestion: Some("Mods".to_string()),
    });
}
