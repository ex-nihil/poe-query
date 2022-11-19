#[cfg(test)]
mod array {
    use crate::tests::test_util::process;

    #[test]
    fn create_array_empty() {
        let result = process("[]");
        assert_eq!(result, vec!["[]"]);
    }

    #[test]
    fn create_array() {
        let result = process("[0, 1, 2]");
        assert_eq!(result, vec!["[0,1,2]"]);
    }

    #[test]
    fn create_array_one_element() {
        let result = process("[0]");
        assert_eq!(result, vec!["[0]"]);
    }

    #[test]
    fn array_length() {
        let result = process("[0,1,2,3] | length");
        assert_eq!(result, vec!["4"]);
    }

}