use std::fmt::Debug;

use log::{debug, trace};
use pest::error::LineColLocation;
use pest::Parser;

use crate::error::QueryError;

#[derive(Parser)]
#[grammar = "query/grammar.pest"]
struct PluckParser;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Term {
    LookupByName(String),
    LookupKeyValueByName(String),
    LookupByIndex(usize),
    ByIndexReverse(usize),
    SliceData(i64, i64),
    KeyValue(Box<Term>, Vec<Term>),
    ObjectConstruction(Vec<Term>),
    BoolLiteral(bool),
    ArrayConstruction(Vec<Term>),
    Select(Vec<Term>, Option<Compare>, Vec<Term>),
    Calculate(Vec<Term>, Operation, Vec<Term>),
    Iterator,
    StringLiteral(String),
    Key(Vec<Term>),
    SetVariable(String),
    GetVariable(String),
    Contains(Vec<Term>),
    UnsignedNumber(u64),
    Reduce(Vec<Term>, Vec<Term>, Vec<Term>),
    Map(Vec<Term>),
    SignedNumber(i64),
    Transpose,
    Identity,
    CommaSeparator,
    Length,
    Keys,
    NoOperation,
    PipeOperator,
    _Equal,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Compare {
    Equals,
    NotEquals,
    LessThan,
    GreaterThan,
    LessThanEq,
    GreaterThanEq,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Operation {
    Addition,
    Subtraction,
    Multiplication,
    Division,
}

pub fn parse_query(source: &str) -> Result<Vec<Term>, QueryError> {
    let pairs = match PluckParser::parse(Rule::program, source) {
        Ok(pairs) => pairs,
        Err(error) => {
            let (line, column) = match error.line_col {
                LineColLocation::Pos((line, column)) => (line, column),
                LineColLocation::Span((line, column), (_, _)) => (line, column),
            };
            return Err(QueryError::Parse { line, column, message: error.to_string() });
        }
    };

    let mut terms = Vec::new();
    for pair in pairs {
        terms.append(&mut build_ast(pair)?);
    }

    debug!("Query terms: {:?}", terms);
    Ok(terms)
}

fn build_ast(pair: pest::iterators::Pair<Rule>) -> Result<Vec<Term>, QueryError> {
    trace!("pair: {:?}", pair);

    match pair.as_rule() {
        Rule::multiple_terms => {
            let mut terms = Vec::new();
            for inner in pair.into_inner() {
                terms.append(&mut build_ast(inner)?);
            }
            Ok(terms)
        }
        Rule::calculation => {
            let mut left_operand = Vec::new();
            let mut right_operand = Vec::new();
            let mut current = &mut left_operand;
            let mut operation = None;
            for next in pair.into_inner() {
                match next.as_rule() {
                    Rule::operation => {
                        operation = match next.into_inner().next().unwrap().as_rule() {
                            Rule::add => Some(Operation::Addition),
                            Rule::subtract => Some(Operation::Subtraction),
                            Rule::multiply => Some(Operation::Multiplication),
                            Rule::divide => Some(Operation::Division),
                            rule => {
                                return Err(QueryError::internal(format!("unexpected rule '{:?}', expected math operation", rule)));
                            }
                        };
                        current = &mut right_operand;
                    }
                    _ => current.append(&mut build_ast(next)?),
                }
            }

            match (operation, left_operand, right_operand) {
                (None, lhs, _) => Ok(lhs),
                (Some(op), lhs, rhs) =>
                    Ok(vec![Term::Calculate(lhs, op, rhs)])
            }
        }
        Rule::zip_to_obj => Ok(zip_to_object_terms()),
        _ => Ok(vec![to_term(pair)?])
    }
}

fn zip_to_object_terms() -> Vec<Term> {
    vec![
        Term::Transpose,
        Term::Map(vec![
            Term::ObjectConstruction(vec![
                Term::KeyValue(Box::new(Term::Key(vec![Term::LookupByIndex(0)])), vec![Term::LookupByIndex(1)])
            ])
        ]),
        Term::Reduce(
            vec![
                Term::Identity,
                Term::Iterator,
                Term::SetVariable("item".to_string()),
            ],
            vec![Term::ObjectConstruction(vec![])],
            vec![
                Term::Calculate(
                    vec![Term::Identity],
                    Operation::Addition,
                    vec![Term::GetVariable("item".to_string())],
                )
            ],
        ),
    ]
}

fn parse_number<T: std::str::FromStr>(text: &str) -> Result<T, QueryError> {
    text.parse::<T>()
        .map_err(|_| QueryError::internal(format!("number literal '{}' out of range", text)))
}

fn to_term(pair: pest::iterators::Pair<Rule>) -> Result<Term, QueryError> {
    trace!("{:?}", pair.as_rule());
    let term = match pair.as_rule() {
        Rule::EOI => Term::NoOperation,
        Rule::pipe => Term::PipeOperator,
        Rule::iterator => Term::Iterator,
        Rule::identity => Term::Identity,
        Rule::comma => Term::CommaSeparator,
        Rule::length => Term::Length,
        Rule::keys => Term::Keys,
        Rule::transpose => Term::Transpose,
        Rule::field => Term::LookupByName(pair.as_span().as_str().to_string()),
        Rule::kv_by_field => Term::LookupKeyValueByName(pair.as_span().as_str().to_string()),
        Rule::string => Term::StringLiteral(pair.as_span().as_str().to_string()),
        Rule::identifier => Term::StringLiteral(pair.as_span().as_str().to_string()),

        Rule::assign_variable => {
            let mut inner = pair.into_inner();
            let text = inner.next().unwrap().into_inner().as_str();
            Term::SetVariable(text.to_string())
        }
        Rule::variable => {
            let mut inner = pair.into_inner();
            let text = inner.next().unwrap().as_str();
            Term::GetVariable(text.to_string())
        }
        Rule::key => {
            let mut terms = Vec::new();
            for inner in pair.into_inner() {
                terms.append(&mut build_ast(inner)?);
            }
            Term::Key(terms)
        }
        Rule::index => {
            let ident = pair.into_inner().next().unwrap().as_str();
            let index: i64 = parse_number(ident)?;
            if index < 0 {
                Term::ByIndexReverse(-index as usize)
            } else {
                Term::LookupByIndex(index as usize)
            }
        }
        Rule::map => {
            let mut terms = Vec::new();
            for inner in pair.into_inner() {
                terms.append(&mut build_ast(inner)?);
            }
            Term::Map(terms)
        }
        Rule::signed_number => {
            let mut inner = pair.into_inner();
            let Some(next) = inner.next() else {
                return Err(QueryError::internal("parsing failed on signed_number, this is a bug in the language spec"));
            };

            match next.as_rule() {
                Rule::minus => {
                    let value: i64 = parse_number(inner.next().unwrap().as_str())?;
                    Term::SignedNumber(-value)
                }
                _ => {
                    let value: i64 = parse_number(next.as_str())?;
                    Term::SignedNumber(value)
                }
            }
        }
        Rule::unsigned_number => {
            let next = pair.into_inner().next().unwrap();
            let value: u64 = parse_number(next.as_str())?;
            Term::UnsignedNumber(value)
        }
        Rule::select => {
            let inner = pair.into_inner();
            let mut lhs = Vec::new();
            let mut rhs = Vec::new();
            let mut current = &mut lhs;
            let mut comparison = None;
            for next in inner {
                match next.as_rule() {
                    Rule::bool_constant => {
                        let bool = match next.into_inner().next().unwrap().as_rule() {
                            Rule::TRUE => Term::BoolLiteral(true),
                            _ => Term::BoolLiteral(false),
                        };
                        return Ok(Term::Select(vec![bool], None, vec![]));
                    }
                    Rule::compare => {
                        comparison = match next.into_inner().next().unwrap().as_rule() {
                            Rule::equal => Some(Compare::Equals),
                            Rule::not_equal => Some(Compare::NotEquals),
                            Rule::less_than => Some(Compare::LessThan),
                            Rule::greater_than => Some(Compare::GreaterThan),
                            Rule::less_than_eq => Some(Compare::LessThanEq),
                            Rule::greater_than_eq => Some(Compare::GreaterThanEq),
                            rule => {
                                return Err(QueryError::internal(format!("unexpected rule '{:?}', expected comparison operation", rule)));
                            }
                        };
                        current = &mut rhs;
                    }
                    _ => current.push(to_term(next)?),
                }
            }
            Term::Select(lhs, comparison, rhs)
        }
        Rule::contains => {
            let inner = pair.into_inner();
            let inner_terms = inner.map(to_term).collect::<Result<Vec<_>, _>>()?;
            Term::Contains(inner_terms)
        }
        Rule::slice => {
            let mut inner = pair.into_inner();
            let mut from = 0;
            let mut to = i64::MAX;
            if let Some(first) = inner.next() {
                match first.as_rule() {
                    Rule::slice_from => from = parse_number(first.into_inner().as_str())?,
                    Rule::slice_to => to = parse_number(first.into_inner().as_str())?,
                    _ => {}
                }
            }
            if let Some(first) = inner.next() {
                match first.as_rule() {
                    Rule::slice_from => from = parse_number(first.into_inner().as_str())?,
                    Rule::slice_to => to = parse_number(first.into_inner().as_str())?,
                    _ => {}
                }
            }
            Term::SliceData(from, to)
        }
        Rule::array_construction => {
            let content = pair.into_inner();
            let mut items = Vec::new();
            for next in content {
                items.push(to_term(next)?);
            }
            Term::ArrayConstruction(items)
        }
        Rule::object_construct => {
            let inner = pair.into_inner();
            let mut object_terms = Vec::new();
            for pair in inner {
                match pair.as_rule() {
                    Rule::comma => object_terms.push(to_term(pair)?),
                    Rule::kv_by_field => object_terms.push(to_term(pair)?),
                    Rule::key_value => {
                        let mut terms = Vec::new();
                        for inner in pair.into_inner() {
                            terms.append(&mut build_ast(inner)?);
                        }
                        let Some(key) = terms.first() else {
                            return Err(QueryError::internal("object construction key/value pair without a key"));
                        };
                        object_terms.push(Term::KeyValue(Box::new(key.clone()), terms[1..].to_vec()));
                    }
                    rule => {
                        return Err(QueryError::internal(format!("unexpected rule '{:?}' during object construction", rule)));
                    }
                }
            }
            Term::ObjectConstruction(object_terms)
        }
        Rule::reduce => {
            let inner = pair.into_inner();

            let mut initial = Vec::<Term>::new();
            let mut inner_terms = Vec::<Term>::new();
            let mut outer_terms = Vec::<Term>::new();
            let mut current = &mut outer_terms;
            for next in inner {
                match next.as_rule() {
                    Rule::reduce_init_value => {
                        current = &mut inner_terms;
                        let Some(inner_next) = next.into_inner().next() else {
                            return Err(QueryError::internal("reduce is missing its initial value"));
                        };
                        initial.append(&mut build_ast(inner_next)?);
                    }
                   _ => current.append(&mut build_ast(next)?)
                }
            }
            Term::Reduce(outer_terms, initial, inner_terms)
        }
        unexpected_rule => {
            return Err(QueryError::internal(format!("rule from language spec not implemented: {:?}", unexpected_rule)));
        }
    };
    Ok(term)
}
