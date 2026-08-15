mod common;

use common::store::example_store;
use poe_query_lib::error::QueryError;
use poe_query_lib::query;
use poe_query_lib::traversal::{QueryProcessor, StaticContext, value::Value};

fn run(input: &str) -> Result<Value, QueryError> {
    let store = example_store();
    let terms = query::parse_query(input)?;
    StaticContext::new(&store).process(&terms)
}

fn run_json(input: &str) -> String {
    serde_json::to_string(&run(input).unwrap()).unwrap()
}

#[test]
fn reads_fields_from_synthetic_table() {
    assert_eq!(run_json(".Mods[0].Id"), "\"mod_a\"");
    assert_eq!(run_json(".Mods[1].Id"), "\"mod_b\"");
    assert_eq!(run_json(".Mods[1].Weight"), "50");
    assert_eq!(run_json(".Mods[0] | keys"), "[\"Id\",\"Weight\",\"ModTypeKey\"]");
}

#[test]
fn follows_foreign_keys() {
    assert_eq!(run_json(".Mods[0].ModTypeKey.Name"), "\"type_b\"");
    assert_eq!(run_json(".Mods[1].ModTypeKey.Name"), "\"type_a\"");
}

#[test]
fn unknown_table_is_an_error_with_suggestion() {
    let error = run(".Modz[0].Id").unwrap_err();
    assert_eq!(error, QueryError::UnknownTable {
        name: "Modz".to_string(),
        suggestion: Some("Mods".to_string()),
    });
    assert_eq!(error.to_string(), "unknown table 'Modz'. Did you mean 'Mods'?");
}

#[test]
fn unknown_table_without_a_close_match() {
    let error = run(".Frobnicators[0]").unwrap_err();
    assert_eq!(error, QueryError::UnknownTable {
        name: "Frobnicators".to_string(),
        suggestion: None,
    });
    assert_eq!(error.to_string(), "unknown table 'Frobnicators'");
}

#[test]
fn unknown_column_is_an_error_with_suggestion() {
    let error = run(".Mods[0].Idz").unwrap_err();
    assert_eq!(error, QueryError::UnknownColumn {
        table: "Mods".to_string(),
        column: "Idz".to_string(),
        suggestion: Some("Id".to_string()),
    });
    assert_eq!(error.to_string(), "table 'Mods' has no column 'Idz'. Did you mean 'Id'?");
}

#[test]
fn unknown_column_when_iterating_rows() {
    let error = run(".Mods[].Weightz").unwrap_err();
    assert!(matches!(error, QueryError::UnknownColumn { suggestion: Some(ref s), .. } if s == "Weight"));
}

#[test]
fn missing_data_file_is_an_error() {
    let error = run(".Ghost[0].Id").unwrap_err();
    assert_eq!(error, QueryError::MissingDataFile { table: "Ghost".to_string() });
}

#[test]
fn missing_key_on_constructed_object_stays_null() {
    // jq semantics for JSON built inside the query: absent keys yield null
    let result = run("{ foo: 1 } | .bar").unwrap();
    assert_eq!(serde_json::to_string(&result).unwrap(), "null");
}
