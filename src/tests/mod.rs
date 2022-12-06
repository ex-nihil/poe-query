mod addition;
mod subtraction;
mod misc;
mod object;
mod arrays;
mod not_implemented;
mod string;


#[cfg(test)]
pub mod test_util {
    use crate::{query, Term, Value};
    use crate::traversal::{StaticContext, QueryProcessor};

    pub fn process(input: &str) -> Vec<String> {
        println!("Input: {}", input);
        let terms = query::parse_query(input).unwrap();

        print_terms(&terms, 0);

        let value = StaticContext::default().process(&terms);

        match value {
            Value::Iterator(items) => {
                items.iter().map(|item| {
                    serde_json::to_string(item).expect("seralized")
                }).collect()
            }
            _ => vec![serde_json::to_string(&value).expect("serialized")]
        }
    }

    pub fn print_terms(terms: &Vec<Term>, indentation: u8) {
        terms.iter().for_each(|term| {
            match term {
                Term::Calculate(lhs, op, rhs) => {
                    indent(indentation); println!("{:?} (", op);
                    print_terms(lhs, indentation + 1);
                    print_terms(rhs, indentation + 1);
                    indent(indentation); println!(")");
                }
                Term::ObjectConstruction(inner) => {
                    indent(indentation); println!("obj {{");
                    print_terms(inner, indentation + 1);
                    indent(indentation); println!("}}");
                },
                Term::ArrayConstruction(inner) => {
                    indent(indentation); println!("array [");
                    print_terms(inner, indentation + 1);
                    indent(indentation); println!("]");
                }
                _ => {
                    indent(indentation);
                    println!("{:?}", term);
                }
            }
        });
    }

    fn indent(levels: u8) {
        for _ in 0..levels {
            print!("\t");
        }
    }
}