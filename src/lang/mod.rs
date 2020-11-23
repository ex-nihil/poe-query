use pest::Parser;
use std::fmt::Debug;

#[derive(Parser)]
#[grammar = "lang/grammar.pest"]
struct PluckParser;

#[derive(Debug, PartialEq, Clone)]
#[allow(non_camel_case_types)]
pub enum Term {
    by_name(String),
    by_index(u64),
    by_index_reverse(u64),
    kv(String, Vec<Term>),
    object(Vec<Term>),
    pipe,
    comma,
    noop,
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
        //println!("pair: {:?}", pair);
        build_ast(pair, &mut output);
        //output.push();
    }
    return output;
}
// TODO: this shit must be moved to navigator so it can store objects created as current value
fn build_ast(pair: pest::iterators::Pair<Rule>, dst: &mut Vec<Term>) {
    match pair.as_rule() {
        Rule::object_construct => {
            //println!("OMEGALUL {:?}", pair);
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
        Rule::select => {
            dst.push(to_term(pair));
        }
        Rule::array_index => {
            dst.push(to_term(pair));
        }
        Rule::array_index_reverse => {
            dst.push(to_term(pair));
        }
        Rule::pipe => dst.push(to_term(pair)),
        Rule::comma => dst.push(to_term(pair)),
        _ => {
            println!("unhandled pair {:?}", pair);
        }
    }
}

fn to_term(pair: pest::iterators::Pair<Rule>) -> Term {
    match pair.as_rule() {
        Rule::select => {
            let ident = pair.as_span().as_str().to_string();
            Term::by_name(ident.to_string())
        }
        Rule::array_index => {
            let ident = pair.into_inner().next().unwrap().as_str();
            let index = ident.parse::<u64>().unwrap();
            Term::by_index(index)
        }
        Rule::array_index_reverse => {
            let ident = pair.into_inner().next().unwrap().as_str();
            let index = ident.parse::<u64>().unwrap();
            Term::by_index_reverse(index)
        }
        Rule::pipe => Term::pipe,
        Rule::comma => Term::comma,
        _ => {
            println!("UNHANDLED: {}", pair);
            Term::noop
        }
    }
}
