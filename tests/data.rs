use schematic::Interpreter;

fn eval(src: &str) -> String {
    match Interpreter::new().eval_str(src) {
        Ok(v) => v.to_string(),
        Err(e) => format!("error: {e}"),
    }
}

#[test]
fn arrays_are_built_from_collections_and_infer_their_type() {
    assert_eq!(eval("(array [1 2 3])"), "#[1 2 3]");
    assert_eq!(eval("(array [1 2.5])"), "#[1.0 2.5]");
    assert_eq!(eval("(array [true false])"), "#[true false]");
    assert_eq!(eval("(array [\"a\" \"b\"])"), "#[\"a\" \"b\"]");
    assert_eq!(eval("(array [1 nil 3])"), "#[1 nil 3]");
    assert_eq!(eval("(array [])"), "#[]");
    assert_eq!(eval("(array '(1 2))"), "#[1 2]");
    assert_eq!(eval("(array [1 \"a\"])"), "error: array elements must all be numbers, all booleans, or all strings, got 1 and \"a\"");
    assert_eq!(
        eval("[(array-type (array [1])) (array-type (array [1.0])) (array-type (array [true])) (array-type (array [\"s\"])) (array-type (array [nil]))]"),
        "[:int :float :bool :string :float]"
    );
}

#[test]
fn array_literals_read_and_print() {
    assert_eq!(eval("#[1 2 3]"), "#[1 2 3]");
    assert_eq!(eval("#[1 (+ 1 1)]"), "#[1 2]");
    assert_eq!(eval("[(array? #[1]) (array? [1])]"), "[true false]");
    assert_eq!(eval("(equal? #[1 2] (array [1 2]))"), "true");
    assert_eq!(eval("[(equal? #[1 2] #[1 3]) (equal? #[1 nil] #[1 nil]) (equal? #[1] [1])]"), "[false true false]");
    assert_eq!(eval("(array (range 25))"), "#[0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 ... 25 items]");
}

#[test]
fn arrays_act_as_sequences() {
    assert_eq!(eval("[(count #[1 2 3]) (nth #[1 2 3] 1) (#[1 2 3] 0) (get #[1 2] 5) (first #[7 8]) (last #[1 2])]"), "[3 2 1 nil 7 2]");
    assert_eq!(eval("[(rest #[7 8 9]) (vec #[1 nil]) (map inc #[1 2]) (reduce + #[1 2 3]) (empty? #[]) (nth #[1 nil] 1)]"), "[(8 9) [1 nil] [2 3] 6 true nil]");
    assert_eq!(eval("[(conj #[1 2] 3) (conj #[1] 2.5) (conj #[1] nil)]"), "[#[1 2 3] #[1.0 2.5] #[1 nil]]");
    assert_eq!(eval("(conj #[1] \"a\")"), "error: array elements must all be numbers, all booleans, or all strings, got 1 and \"a\"");
    assert_eq!(eval("[(count-missing #[1 nil nil]) (count-missing #[])]"), "[2 0]");
    assert_eq!(eval("(sort #[3 1 2])"), "[1 2 3]");
}
