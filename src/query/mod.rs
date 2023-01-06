use std::fmt::Debug;
use std::process;

use log::{debug, error, trace};
use pest::error::LineColLocation;
use pest::Parser;

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

pub fn parse_query(source: &str) -> Result<Vec<Term>, String> {
    let pairs = match PluckParser::parse(Rule::program, source) {
        Ok(pairs) => pairs,
        Err(error) => {
            let parse_error = match error.line_col {
                LineColLocation::Pos((line, column)) =>
                    format!("Error parsing grammar at line {}, column {}. {}", line, column, error),
                LineColLocation::Span((line, column), (line_to, column_to)) =>
                    format!("Error parsing grammar at line {}, column {} to line {}, column {}. {}", line, column, line_to, column_to, error),
            };
            return Err(parse_error);
        }
    };

    let terms = pairs.into_iter()
        .flat_map(build_ast)
        .collect::<Vec<_>>();

    debug!("Query terms: {:?}", terms);
    Ok(terms)
}

fn build_ast(pair: pest::iterators::Pair<Rule>) -> Vec<Term> {
    trace!("pair: {:?}", pair);

    match pair.as_rule() {
        Rule::multiple_terms => {
            pair.into_inner().into_iter()
                .flat_map(build_ast)
                .collect::<Vec<_>>()
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
                                error!("Unexpected rule '{:?}'. Expected math operation.", rule);
                                process::exit(-1);
                            }
                        };
                        current = &mut right_operand;
                    }
                    _ => current.append(&mut build_ast(next)),
                }
            }

            match (operation, left_operand, right_operand) {
                (None, lhs, _) => lhs,
                (Some(op), lhs, rhs) =>
                    vec![Term::Calculate(lhs, op, rhs)]
            }
        }
        Rule::zip_to_obj => zip_to_object_terms(),
        _ => vec![to_term(pair)]
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

fn to_term(pair: pest::iterators::Pair<Rule>) -> Term {
    trace!("{:?}", pair.as_rule());
    match pair.as_rule() {
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
            let inner = pair.into_inner();
            let terms = inner.into_iter()
                .flat_map(build_ast)
                .collect::<Vec<_>>();
            Term::Key(terms)
        }
        Rule::index => {
            let ident = pair.into_inner().next().unwrap().as_str();
            let index = ident.parse::<i64>().unwrap();
            if index < 0 {
                Term::ByIndexReverse(-index as usize)
            } else {
                Term::LookupByIndex(index as usize)
            }
        }
        Rule::map => {
            let inner = pair.into_inner();
            let terms = inner.into_iter()
                .flat_map(build_ast)
                .collect::<Vec<_>>();
            Term::Map(terms)
        }
        Rule::signed_number => {
            let mut inner = pair.into_inner();
            let Some(next) = inner.next() else {
                error!("Parsing failed Rule::signed_number. This is a bug in the language spec.");
                process::exit(-1);
            };

            match next.as_rule() {
                Rule::minus => {
                    let value_string = inner.next().unwrap().as_str();
                    let value = value_string.parse::<i64>().unwrap();
                    Term::SignedNumber(-value)
                }
                _ => {
                    let value = next.as_str().parse::<i64>().unwrap();
                    Term::SignedNumber(value)
                }
            }
        }
        Rule::unsigned_number => {
            let next = pair.into_inner().next().unwrap();
            let value = next.as_str().parse::<u64>().unwrap();
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
                        return Term::Select(vec![bool], None, vec![]);
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
                                error!("Unexpected rule '{:?}'. Expected comparison operation.", rule);
                                process::exit(-1);
                            }
                        };
                        current = &mut rhs;
                    }
                    _ => current.push(to_term(next)),
                }
            }
            Term::Select(lhs, comparison, rhs)
        }
        Rule::contains => {
            let inner = pair.into_inner();
            let inner_terms: Vec<_> = inner.map(to_term).collect();
            Term::Contains(inner_terms)
        }
        Rule::slice => {
            let mut inner = pair.into_inner();
            let mut from = 0;
            let mut to = i64::MAX;
            if let Some(first) = inner.next() {
                match first.as_rule() {
                    Rule::slice_from => {
                        from = first.into_inner().as_str().parse::<i64>().unwrap()
                    }
                    Rule::slice_to => to = first.into_inner().as_str().parse::<i64>().unwrap(),
                    _ => {}
                }
            }
            if let Some(first) = inner.next() {
                match first.as_rule() {
                    Rule::slice_from => {
                        from = first.into_inner().as_str().parse::<i64>().unwrap()
                    }
                    Rule::slice_to => to = first.into_inner().as_str().parse::<i64>().unwrap(),
                    _ => {}
                }
            }
            Term::SliceData(from, to)
        }
        Rule::array_construction => {
            let content = pair.into_inner();
            let mut items = Vec::new();
            for next in content {
                items.push(to_term(next));
            }
            Term::ArrayConstruction(items)
        }
        Rule::object_construct => {
            let inner = pair.into_inner();
            let mut object_terms = Vec::new();
            for pair in inner {
                match pair.as_rule() {
                    Rule::comma => object_terms.push(to_term(pair)),
                    Rule::kv_by_field => object_terms.push(to_term(pair)),
                    Rule::key_value => {
                        let content = pair.into_inner();
                        let terms = content.into_iter()
                            .flat_map(build_ast)
                            .collect::<Vec<_>>();
                        let key = terms.first().unwrap();
                        object_terms.push(Term::KeyValue(Box::new(key.clone()), terms[1..].to_vec()));
                    }
                    rule => {
                        error!("Unexpected rule '{:?}' during object construction ", rule);
                        process::exit(-1);
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
                            error!("Expected a value from iterator, but got None");
                            process::exit(-1);
                        };
                        initial.append(&mut build_ast(inner_next));
                    }
                   _ => current.append(&mut build_ast(next))
                }
            }
            Term::Reduce(outer_terms, initial, inner_terms)
        }
        unexpected_rule => {
            error!("Rule from language spec not implemented: {:?}", unexpected_rule);
            process::exit(-1);
        }
    }
}
