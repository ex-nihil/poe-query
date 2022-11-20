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
    fn iterate() {
        let result = process("[0, 1, 2][]");
        assert_eq!(result, vec!["0", "1", "2"]);
    }

    #[test]
    fn index() {
        let result = process("[5, 6, 7][1]");
        assert_eq!(result[0], "6");
    }

    #[test]
    fn index_negative() {
        let result = process("[5, 6, 7][-1]");
        assert_eq!(result[0], "7");

        let result = process("[5, 6, 7][-0]");
        assert_eq!(result[0], "5");
    }

    #[test]
    fn array_length() {
        let result = process("[0,1,2,3] | length");
        assert_eq!(result, vec!["4"]);
    }

    #[test]
    fn slice() {
        let result = process("[5, 6, 7, 8][1:3]");
        assert_eq!(result[0], "[6,7]");
    }

    #[test]
    fn slice_negative_indices() {
        let result = process("[0, 1, 2, 3, 4, 5, 6] | .[-3:-1]");
        assert_eq!(result[0], "[4,5]");
    }

    #[test]
    fn slice_invalid_order_return_empty_array() {
        let result = process("[0, 1, 2, 3, 4, 5, 6] | .[-1:-3]");
        assert_eq!(result[0], "[]");
    }
}