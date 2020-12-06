use pest::Parser;
use std::fmt::Debug;

#[derive(Parser)]
#[grammar = "lang/grammar.pest"]
struct PluckParser;

#[derive(Debug, PartialEq, Clone)]
#[allow(non_camel_case_types)]
pub enum Term {
    by_name(String),
    by_index(usize),
    by_index_reverse(usize),
    slice(usize,usize),
    kv(String, Vec<Term>),
    object(Vec<Term>),
    select(Vec<Term>, Compare, Vec<Term>),
    iterator,
    equal,
    pipe,
    string(String),
    unsigned_number(u64),
    signed_number(i64),
    comma,
    noop,
}

#[derive(Debug, PartialEq, Clone)]
#[allow(non_camel_case_types)]
pub enum Compare {
    equals,
    not_equals,
    less_than,
    greater_than,
}

pub fn parse(source: &str) -> Vec<Term> {
    let result = PluckParser::parse(Rule::program, source);
    let pairs = match result {
        Ok(pairs) => pairs,
        Err(err) => {
            println!("{}", err);
            panic!("parse error");
        }
    };

    let mut output = Vec::new();
    for pair in pairs {
        build_ast(pair, &mut output);
    }
    return output;
}

// TODO: this shit must be moved to navigator so it can store objects created as current value
fn build_ast(pair: pest::iterators::Pair<Rule>, dst: &mut Vec<Term>) {
    match pair.as_rule() {
        Rule::object_construct => {
            let content = pair.into_inner();
            let mut object_terms = Vec::new();
            for pair in content {
                match pair.as_rule() {
                    Rule::comma => object_terms.push(to_term(pair)),
                    _ => {
                        let mut content = pair.into_inner();
                        let ident = match content.next() {
                            Some(p) => p.into_inner().as_str(),
                            None => panic!("Introduced a new construct without updating term parser?"),
                        };
                        let mut kv_terms = Vec::new();
                        while let Some(next) = content.next() {
                            kv_terms.push(to_term(next));
                        }
                        object_terms.push(Term::kv(ident.to_string(), kv_terms));
                    }
                }
            }
            dst.push(Term::object(object_terms));
        }
        Rule::field => {
            dst.push(to_term(pair));
        }
        Rule::index => {
            dst.push(to_term(pair));
        }
        Rule::index_reverse => {
            dst.push(to_term(pair));
        }
        Rule::iterator => dst.push(to_term(pair)),
        Rule::select => dst.push(to_term(pair)),
        Rule::slice => dst.push(to_term(pair)),
        Rule::string => dst.push(to_term(pair)),
        Rule::signed_number => dst.push(to_term(pair)),
        Rule::unsigned_number => dst.push(to_term(pair)),
        Rule::pipe => {},
        Rule::comma => dst.push(to_term(pair)),
        _ => {
            println!("unhandled pair {:?}", pair);
        }
    }
}

fn to_term(pair: pest::iterators::Pair<Rule>) -> Term {
    match pair.as_rule() {
        Rule::field => {
            let ident = pair.as_span().as_str().to_string();
            Term::by_name(ident.to_string())
        }
        Rule::string => {
            let text = pair.as_span().as_str().to_string();
            println!("STRING {:?}", pair);
            Term::string(text)
        }
        Rule::index => {
            let ident = pair.into_inner().next().unwrap().as_str();
            let index = ident.parse::<usize>().unwrap();
            Term::by_index(index)
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
                    },
                    _ => {
                        let value = next.as_str().parse::<i64>().unwrap();
                        Term::signed_number(value)
                    }
                }
            } else {
                panic!("Parsing failed Rule::signed_number. This is a bug in the language spec.");
            }
        },
        Rule::unsigned_number => {
            let next = pair.into_inner().next().unwrap();
            let value = next.as_str().parse::<u64>().unwrap();
            Term::unsigned_number(value)
        },
        Rule::select => {
            let inner = pair.into_inner();
            let mut lhs = Vec::new();
            let mut rhs = Vec::new();
            let mut current = &mut lhs;
            let mut comparison = Compare::not_equals;
            for next in inner {
                match next.as_rule() {
                    Rule::compare => {
                        comparison = match next.into_inner().next().unwrap().as_rule() {
                            Rule::equal => Compare::equals,
                            Rule::not_equal => Compare::not_equals,
                            Rule::less_than => Compare::less_than,
                            Rule::greater_than => Compare::greater_than,
                            p => panic!(format!("Operation not implemented: {:?}", p)),
                        };
                        current = &mut rhs;
                    }
                    _ => current.push(to_term(next)),
                }
            };
            Term::select(lhs, comparison, rhs)
        }
        Rule::index_reverse => {
            let ident = pair.into_inner().next().unwrap().as_str();
            let index = ident.parse::<usize>().unwrap();
            Term::by_index_reverse(index)
        }
        Rule::slice => {
            let mut inner = pair.into_inner();
            let mut from = 0 as usize;
            let mut to = usize::MAX;
            if let Some(first) = inner.next() {
                match first.as_rule() {
                    Rule::slice_from => from = first.into_inner().as_str().parse::<usize>().unwrap(),
                    Rule::slice_to => to = first.into_inner().as_str().parse::<usize>().unwrap(),
                    _ => {}
                }
            }
            if let Some(first) = inner.next() {
                match first.as_rule() {
                    Rule::slice_from => from = first.into_inner().as_str().parse::<usize>().unwrap(),
                    Rule::slice_to => to = first.into_inner().as_str().parse::<usize>().unwrap(),
                    _ => {}
                }
            }
            Term::slice(from, to)
        }
        Rule::pipe => Term::pipe,
        Rule::comma => Term::comma,
        _ => {
            println!("UNHANDLED: {}", pair);
            Term::noop
        }
    }
}
