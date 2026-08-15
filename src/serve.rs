use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::time::Instant;

use log::*;
use serde::{Deserialize, Serialize};

use crate::dat::DatReader;
use crate::error::QueryError;
use crate::introspect;
use crate::query;
use crate::translate::{StatDescriptions, StatValue};
use crate::traversal::{QueryProcessor, SharedCache, StaticContext};

/// NDJSON protocol over stdio: one request per line on stdin, one response
/// per line on stdout. Logs stay on stderr. Single-threaded; requests are
/// answered in order. EOF on stdin ends the session.
///
///   {"id": 1, "method": "query",    "params": {"query": ".Mods[0].Id"}}
///   {"id": 2, "method": "tables"}
///   {"id": 3, "method": "describe", "params": {"table": "Mods"}}
///   {"id": 4, "method": "ping"}
#[derive(Deserialize)]
struct Request {
    #[serde(default)]
    id: serde_json::Value,
    method: String,
    #[serde(default)]
    params: Params,
}

#[derive(Deserialize, Default)]
struct Params {
    query: Option<String>,
    table: Option<String>,
    stats: Option<Vec<StatParam>>,
    texts: Option<Vec<String>>,
    file: Option<String>,
}

#[derive(Deserialize)]
struct StatParam {
    id: String,
    value: Option<f64>,
    min: Option<f64>,
    max: Option<f64>,
}

#[derive(Serialize)]
struct Response {
    id: serde_json::Value,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ErrorBody>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timings: Option<Timings>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct ErrorBody {
    kind: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggestion: Option<String>,
}

#[derive(Serialize)]
struct Timings {
    parse_ms: u128,
    eval_ms: u128,
}

impl Response {
    fn ok(id: serde_json::Value, result: serde_json::Value, timings: Option<Timings>) -> Self {
        Response { id, ok: true, result: Some(result), error: None, timings, warnings: Vec::new() }
    }

    fn error(id: serde_json::Value, error: ErrorBody) -> Self {
        Response { id, ok: false, result: None, error: Some(error), timings: None, warnings: Vec::new() }
    }

    fn query_error(id: serde_json::Value, error: QueryError) -> Self {
        let suggestion = match &error {
            QueryError::UnknownTable { suggestion, .. } => suggestion.clone(),
            QueryError::UnknownColumn { suggestion, .. } => suggestion.clone(),
            _ => None,
        };
        Self::error(id, ErrorBody {
            kind: error.kind().to_string(),
            message: error.to_string(),
            suggestion,
        })
    }

    fn bad_request(id: serde_json::Value, message: impl Into<String>) -> Self {
        Self::error(id, ErrorBody {
            kind: "bad_request".to_string(),
            message: message.into(),
            suggestion: None,
        })
    }
}

pub fn serve(container: &DatReader, install_path: &Path) {
    let context = StaticContext::new(container);
    let mut cache = SharedCache::default();
    let mut translations: HashMap<String, StatDescriptions> = HashMap::new();

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut output = stdout.lock();

    info!("Serving NDJSON requests on stdin, one per line");
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }

        let response = handle_line(&line, &context, &mut cache, &mut translations, container, install_path);
        match serde_json::to_string(&response) {
            Ok(serialized) => {
                if writeln!(output, "{}", serialized).and_then(|_| output.flush()).is_err() {
                    break;
                }
            }
            Err(error) => error!("failed to serialize response: {}", error),
        }
    }
}

fn handle_line(
    line: &str,
    context: &StaticContext,
    cache: &mut SharedCache,
    translations: &mut HashMap<String, StatDescriptions>,
    container: &DatReader,
    install_path: &Path,
) -> Response {
    let request: Request = match serde_json::from_str(line) {
        Ok(request) => request,
        Err(error) => {
            return Response::bad_request(serde_json::Value::Null, format!("malformed request: {}", error));
        }
    };
    let id = request.id.clone();

    match request.method.as_str() {
        "query" => {
            let Some(query_text) = request.params.query else {
                return Response::bad_request(id, "method 'query' requires params.query");
            };
            handle_query(id, &query_text, context, cache)
        }
        "tables" => {
            let names = introspect::table_names(container.specs());
            match serde_json::to_value(names) {
                Ok(result) => Response::ok(id, result, None),
                Err(error) => Response::bad_request(id, error.to_string()),
            }
        }
        "describe" => {
            let Some(table) = request.params.table else {
                return Response::bad_request(id, "method 'describe' requires params.table");
            };
            match introspect::describe(container.specs(), &table) {
                Ok(description) => match serde_json::to_value(description) {
                    Ok(result) => Response::ok(id, result, None),
                    Err(error) => Response::bad_request(id, error.to_string()),
                },
                Err(error) => Response::query_error(id, error),
            }
        }
        "translate" => {
            let Some(stat_params) = request.params.stats else {
                return Response::bad_request(id, "method 'translate' requires params.stats");
            };
            let mut stats = Vec::with_capacity(stat_params.len());
            for param in stat_params {
                let (min, max) = match (param.value, param.min, param.max) {
                    (Some(value), _, _) => (value, value),
                    (None, Some(min), Some(max)) => (min, max),
                    _ => return Response::bad_request(id,
                        format!("stat '{}' needs either 'value' or 'min' and 'max'", param.id)),
                };
                stats.push(StatValue { id: param.id, min, max });
            }

            let file = request.params.file.unwrap_or_else(|| "stat_descriptions".to_string());
            let descriptions = match cached_descriptions(translations, container, &file) {
                Ok(descriptions) => descriptions,
                Err(error) => return Response::query_error(id, error),
            };
            let translation = descriptions.translate(&stats, container.language());
            match serde_json::to_value(translation) {
                Ok(result) => Response::ok(id, result, None),
                Err(error) => Response::bad_request(id, error.to_string()),
            }
        }
        "untranslate" => {
            let Some(texts) = request.params.texts else {
                return Response::bad_request(id, "method 'untranslate' requires params.texts");
            };
            let file = request.params.file.unwrap_or_else(|| "stat_descriptions".to_string());
            let descriptions = match cached_descriptions(translations, container, &file) {
                Ok(descriptions) => descriptions,
                Err(error) => return Response::query_error(id, error),
            };
            let results: Vec<serde_json::Value> = texts.iter().map(|text| {
                let matches = descriptions.reverse(text, container.language());
                serde_json::json!({ "text": text, "matches": matches })
            }).collect();
            Response::ok(id, serde_json::Value::Array(results), None)
        }
        "ping" => {
            let result = serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"),
                "install": install_path.to_string_lossy(),
                "tables": container.specs().len(),
            });
            Response::ok(id, result, None)
        }
        unknown => Response::bad_request(id, format!("unknown method '{}'", unknown)),
    }
}

/// Load and cache a parsed stat description file for the session.
fn cached_descriptions<'a>(
    translations: &'a mut HashMap<String, StatDescriptions>,
    container: &DatReader,
    file: &str,
) -> Result<&'a StatDescriptions, QueryError> {
    if !translations.contains_key(file) {
        let descriptions = container.stat_descriptions(Some(file))?;
        translations.insert(file.to_string(), descriptions);
    }
    Ok(&translations[file])
}

fn handle_query(
    id: serde_json::Value,
    query_text: &str,
    context: &StaticContext,
    cache: &mut SharedCache,
) -> Response {
    let now = Instant::now();
    let terms = match query::parse_query(query_text) {
        Ok(terms) => terms,
        Err(error) => return Response::query_error(id, error),
    };
    let parse_ms = now.elapsed().as_millis();

    let now = Instant::now();
    let result = context.process_with_cache(cache, &terms);
    let eval_ms = now.elapsed().as_millis();
    let warnings = cache.take_warnings();

    let result = match result {
        Ok(value) => value,
        Err(error) => return Response::query_error(id, error),
    };

    match serde_json::to_value(&result) {
        Ok(result) => {
            let mut response = Response::ok(id, result, Some(Timings { parse_ms, eval_ms }));
            response.warnings = warnings;
            response
        }
        Err(error) => Response::query_error(id, QueryError::internal(error.to_string())),
    }
}
