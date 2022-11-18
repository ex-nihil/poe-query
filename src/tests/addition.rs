
#[cfg(test)]
mod addition {
    use crate::tests::test_util::process;

    /**
    The operator + takes two filters, applies them both to the same input, and adds the results together. What "adding" means depends on the types involved:

    Numbers are added by normal arithmetic.
    Arrays are added by being concatenated into a larger array.
    Strings are added by being joined into a larger string.
    Objects are added by merging, that is, inserting all the key-value pairs from both objects into a single combined object. If both objects contain a value for the same key, the object on the right of the + wins. (For recursive merge use the * operator.)

    null can be added to any value, and returns the other value unchanged.
     */

    #[test]
    fn numbers() {
        let result = process("1 + 1");
        assert_eq!(result[0], "2");
    }

    #[test]
    fn strings() {
        let result = process(r#""1" + "1""#);
        assert_eq!(result, vec![r#""11""#]);
    }

    #[test]
    fn arrays() {
        // BUG: interpret rhs as an index
        let result = process("[0] + [1]");
        assert_eq!(result, vec!["[0,1]"]);
    }

    #[test]
    fn objects() {
        // BUG: does not overwrite value
        let result = process("{foo: 0, bar: 1} + {foo: 1, baz: 1}");
        assert_eq!(result, vec![r#"{"foo":1,"bar":1,"baz":1}"#]);
    }

}