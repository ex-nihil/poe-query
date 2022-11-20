use pest::Parser;
use std::fmt::Debug;
use log::{debug, error, trace};

#[derive(Parser)]
#[grammar = "query/grammar.pest"]
struct PluckParser;

#[derive(Debug, PartialEq, Eq, Clone)]
#[allow(non_camel_case_types)]
pub enum Term {
    by_name(String),
    kv_by_name(String),
    by_index(usize),
    by_index_reverse(usize),
    slice(i64, i64),
    kv(Box<Term>, Vec<Term>),
    object(Vec<Term>),
    array(Vec<Term>),
    select(Vec<Term>, Compare, Vec<Term>),
    calculate(Vec<Term>, Operation, Vec<Term>),
    iterator,
    string(String),
    key(Vec<Term>),
    set_variable(String),
    get_variable(String),
    unsigned_number(u64),
    reduce(Vec<Term>, Vec<Term>, Vec<Term>),
    map(Vec<Term>),
    signed_number(i64),
    transpose,
    identity,
    comma,
    length,
    keys,
    noop,
    _pipe,
    _equal,
}

#[derive(Debug, PartialEq, Eq, Clone)]
#[allow(non_camel_case_types)]
pub enum Compare {
    equals,
    not_equals,
    less_than,
    greater_than,
    less_than_eq,
    greater_than_eq,
}

#[derive(Debug, PartialEq, Eq, Clone)]
#[allow(non_camel_case_types)]
pub enum Operation {
    add,
    subtract,
    multiply,
    divide,
}

pub fn parse(source: &str) -> Vec<Term> {
    let result = PluckParser::parse(Rule::program, source);
    let pairs = match result {
        Ok(pairs) => pairs,
        Err(err) => {
            error!("Fail parsing grammar. {}", err);
            return vec![];
        }
    };

    let mut output = Vec::new();
    for pair in pairs {
        build_ast(pair, &mut output);
    }
    debug!("Query Terms: {:?}", output);
    output
}

// TODO: this is getting very unwieldy, refactor to something more ergonomic?
fn build_ast(pair: pest::iterators::Pair<Rule>, dst: &mut Vec<Term>) {
    trace!("{:?}", pair.as_rule());
    match pair.as_rule() {
        Rule::multiple_terms => {
            let inner = pair.into_inner();
            for inner_term in inner {
                build_ast(inner_term, dst);
            }
        }
        Rule::calculation => {
            let inner = pair.into_inner();
            let mut lhs = Vec::new();
            let mut rhs = Vec::new();
            let mut current = &mut lhs;
            let mut operation = None;
            for next in inner {
                match next.as_rule() {
                    Rule::operation => {
                        operation = match next.into_inner().next().unwrap().as_rule() {
                            Rule::add => Some(Operation::add),
                            Rule::subtract => Some(Operation::subtract),
                            Rule::multiply => Some(Operation::multiply),
                            Rule::divide => Some(Operation::divide),
                            _ => panic!("Unimplemented operation"),
                        };
                        current = &mut rhs;
                    }
                    _ => {
                        build_ast(next, current);
                    },
                }
            }
            if let Some(op) = operation {
                dst.push(Term::calculate(lhs, op, rhs));
            } else {
                for t in lhs {
                    dst.push(t)
                }
            }
        }
        Rule::zip_to_obj => {
            let instructions = vec![
                Term::transpose, 
                Term::map(vec![
                    Term::object(vec![
                        Term::kv(Box::new(Term::key(vec![Term::by_index(0)])), vec![Term::by_index(1)])
                    ])
                ]),
                Term::reduce(
                    vec![
                        Term::identity,
                        Term::iterator,
                        Term::set_variable("item".to_string()),
                    ],
                    vec![Term::object(vec![])],
                    vec![
                        Term::calculate(
                            vec![Term::identity],
                            Operation::add,
                            vec![Term::get_variable("item".to_string())]
                        )
                    ]
                ),
            ];
            for t in instructions {
                dst.push(t)
            }
        }
        Rule::reduce => {
            let inner = pair.into_inner();

            let mut initial = Vec::new();
            let mut inner_terms = Vec::new();
            let mut outer_terms = Vec::new();
            let mut current = &mut outer_terms;
            for next in inner {
                match next.as_rule() {
                    Rule::reduce_init_value => {
                        current = &mut inner_terms;
                        build_ast(next.into_inner().next().unwrap(), &mut initial);
                    }
                    _ => {
                        build_ast(next, current);
                    }
                }
            }
            dst.push(Term::reduce(outer_terms, initial, inner_terms));
        }
        Rule::map => {
            let inner = pair.into_inner();

            let mut terms = Vec::new();
            for next in inner {
                build_ast(next, &mut terms);
            }
            dst.push(Term::map(terms));
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
                        let mut kv_terms = Vec::new();
                        for next in content {
                            build_ast(next, &mut kv_terms);
                        }
                        let key = kv_terms.first().unwrap();
                        object_terms.push(Term::kv(Box::new(key.clone()), kv_terms[1..].to_vec()));
                    }
                    rule => unimplemented!("Unknown rule '{:?}' in object construction ", rule)
                }
            }
            dst.push(Term::object(object_terms))
        },
        _ => {
            dst.push(to_term(pair));
        },
    }
}

fn to_term(pair: pest::iterators::Pair<Rule>) -> Term {
    trace!("{:?}", pair.as_rule());
    match pair.as_rule() {
        Rule::pipe => {
            //Term::pipe
            Term::noop
        },
        Rule::field => {
            let ident = pair.as_span().as_str().to_string();
            Term::by_name(ident)
        }
        Rule::kv_by_field => {
            let ident = pair.as_span().as_str().to_string();
            Term::kv_by_name(ident)
        }
        Rule::string => {
            let text = pair.as_span().as_str().to_string();
            Term::string(text)
        }
        Rule::identifier => {
            let text = pair.as_span().as_str().to_string();
            Term::string(text)
        }
        Rule::assign_variable => {
            let mut inner = pair.into_inner();
            let text = inner.next().unwrap().into_inner().as_str();
            Term::set_variable(text.to_string())
        }
        Rule::variable => {
            let mut inner = pair.into_inner();
            let text = inner.next().unwrap().as_str();
            Term::get_variable(text.to_string())
        }
        Rule::key => {
            let inner = pair.into_inner();
            let mut terms = Vec::new();
            for next in inner {
                build_ast(next, &mut terms);
            }
            Term::key(terms)
        }
        Rule::index => {
            let ident = pair.into_inner().next().unwrap().as_str();
            let index = ident.parse::<i64>().unwrap();
            if index < 0 {
                Term::by_index_reverse(-index as usize)
            } else {
                Term::by_index(index as usize)
            }
        }
        Rule::iterator => Term::iterator,
        Rule::signed_number => {
            let mut inner = pair.into_inner();
            if let Some(next) = inner.next() {
                match next.as_rule() {
                    Rule::minus => {
                        let value_string = inner.next().unwrap().as_str();
                        let value = value_string.parse::<i64>().unwrap();
                        Term::signed_number(-value)
                    }
                    _ => {
                        let value = next.as_str().parse::<i64>().unwrap();
                        Term::signed_number(value)
                    }
                }
            } else {
                panic!("Parsing failed Rule::signed_number. This is a bug in the language spec.");
            }
        }
        Rule::unsigned_number => {
            let next = pair.into_inner().next().unwrap();
            let value = next.as_str().parse::<u64>().unwrap();
            Term::unsigned_number(value)
        }
        Rule::select => {
            let inner = pair.into_inner();
            let mut lhs = Vec::new();
            let mut rhs = Vec::new();
            let mut current = &mut lhs;
            let mut comparison = Compare::not_equals;
            for next in inner {
                match next.as_rule() {
                    Rule::bool_constant => {
                        comparison = match next.into_inner().next().unwrap().as_rule() {
                            Rule::TRUE => Compare::equals,
                            _ => Compare::not_equals,
                        };
                        return Term::select(vec![Term::unsigned_number(0)], comparison, vec![Term::unsigned_number(0)]);
                    }
                    Rule::compare => {
                        comparison = match next.into_inner().next().unwrap().as_rule() {
                            Rule::equal => Compare::equals,
                            Rule::not_equal => Compare::not_equals,
                            Rule::less_than => Compare::less_than,
                            Rule::greater_than => Compare::greater_than,
                            Rule::less_than_eq => Compare::less_than_eq,
                            Rule::greater_than_eq => Compare::greater_than_eq,
                            p => panic!("Operation not implemented: {:?}", p),
                        };
                        current = &mut rhs;
                    }
                    _ => current.push(to_term(next)),
                }
            }
            Term::select(lhs, comparison, rhs)
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
            Term::slice(from, to)
        }
        Rule::identity => Term::identity,
        Rule::comma => Term::comma,
        Rule::length => Term::length,
        Rule::keys => Term::keys,
        Rule::transpose => Term::transpose,
        Rule::array_construction => {
            let content = pair.into_inner();
            let mut items = Vec::new();
            for next in content {
                items.push(to_term(next));
            }
            Term::array(items)
        }
        Rule::EOI => Term::noop,
        _ => {
            println!("UNHANDLED: {}", pair);
            Term::noop
        }
    }
}
