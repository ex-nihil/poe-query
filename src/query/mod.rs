use pest::Parser;
use std::fmt::Debug;

#[derive(Parser)]
#[grammar = "query/grammar.pest"]
struct PluckParser;

#[derive(Debug, PartialEq, Clone)]
#[allow(non_camel_case_types)]
pub enum Term {
    by_name(String),
    by_index(usize),
    by_index_reverse(usize),
    slice(usize, usize),
    kv(Box<Term>, Vec<Term>),
    object(Vec<Term>),
    array(Vec<Term>),
    select(Vec<Term>, Compare, Vec<Term>),
    calculate(Vec<Term>, Operation, Vec<Term>),
    iterator,
    equal,
    pipe,
    string(String),
    name(Vec<Term>),
    set_variable(String),
    get_variable(String),
    unsigned_number(u64),
    reduce(Vec<Term>, Vec<Term>, Vec<Term>),
    map(Vec<Term>),
    signed_number(i64),
    transpose,
    identity,
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

#[derive(Debug, PartialEq, Clone)]
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

// TODO: this is getting very unwieldy, refactor to something more ergonomic
fn build_ast(pair: pest::iterators::Pair<Rule>, dst: &mut Vec<Term>) {
    match pair.as_rule() {
        Rule::expr => {
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
                        Term::kv(Box::new(Term::name(vec![Term::by_index(0)])), vec![Term::by_index(1)])
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
            let content = pair.into_inner();
            let mut object_terms = Vec::new();
            for pair in content {
                match pair.as_rule() {
                    Rule::comma => object_terms.push(to_term(pair)),
                    _ => {
                        let mut content = pair.into_inner();
                        let mut kv_terms = Vec::new();
                        while let Some(next) = content.next() {
                            build_ast(next, &mut kv_terms);
                        }
                        let key = kv_terms.first().unwrap();
                        object_terms.push(Term::kv(Box::new(key.clone()), kv_terms[1..].to_vec()));
                    }
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
    match pair.as_rule() {
        Rule::pipe => {
            Term::noop
        },
        Rule::field => {
            let ident = pair.as_span().as_str().to_string();
            Term::by_name(ident.to_string())
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
            Term::name(terms)
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
                    Rule::compare => {
                        comparison = match next.into_inner().next().unwrap().as_rule() {
                            Rule::equal => Compare::equals,
                            Rule::not_equal => Compare::not_equals,
                            Rule::less_than => Compare::less_than,
                            Rule::greater_than => Compare::greater_than,
                            p => panic!("Operation not implemented: {:?}", p),
                        };
                        current = &mut rhs;
                    }
                    _ => current.push(to_term(next)),
                }
            }
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
                    Rule::slice_from => {
                        from = first.into_inner().as_str().parse::<usize>().unwrap()
                    }
                    Rule::slice_to => to = first.into_inner().as_str().parse::<usize>().unwrap(),
                    _ => {}
                }
            }
            if let Some(first) = inner.next() {
                match first.as_rule() {
                    Rule::slice_from => {
                        from = first.into_inner().as_str().parse::<usize>().unwrap()
                    }
                    Rule::slice_to => to = first.into_inner().as_str().parse::<usize>().unwrap(),
                    _ => {}
                }
            }
            Term::slice(from, to)
        }
        Rule::identity => Term::identity,
        Rule::comma => Term::comma,
        Rule::transpose => Term::transpose,
        Rule::array_construction => {
            let content = pair.into_inner();
            let mut items = Vec::new();
            for next in content {
                items.push(to_term(next));
            }
            Term::array(items)
        }
        _ => {
            println!("UNHANDLED: {}", pair);
            Term::noop
        }
    }
}
