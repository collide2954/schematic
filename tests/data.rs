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
fn arithmetic_broadcasts_over_arrays() {
    assert_eq!(eval("[(+ #[1 2 3] 1) (* #[1 2] #[10 20]) (- 10 #[1 2]) (* 2 #[1 2 3])]"), "[#[2 3 4] #[10 40] #[9 8] #[2 4 6]]");
    assert_eq!(eval("[(/ #[1 2] 2) (+ #[1 2] 0.5) (- #[1 2]) (+ #[1 2] #[1 2] #[1 2])]"), "[#[0.5 1.0] #[1.5 2.5] #[-1 -2] #[3 6]]");
    assert_eq!(eval("[(+ #[1 nil] 1) (* #[1 2] nil)]"), "[#[2 nil] #[nil nil]]");
    assert_eq!(eval("(+ #[1 2] #[1 2 3])"), "error: array lengths differ: 2 and 3");
    assert_eq!(eval("(+ #[1] \"a\")"), "error: + expects numbers, got \"a\"");
    assert_eq!(eval("(+ #[true] 1)"), "error: + expects numbers, got true");
    assert_eq!(eval("[(quot #[7 8] 2) (mod #[7 8] 3) (pow #[2 3] 2) (sqrt #[4 9]) (abs #[-1 2]) (inc #[1])]"), "[#[3 4] #[1 2] #[4 9] #[2.0 3.0] #[1 2] #[2]]");
    assert_eq!(eval("[(floor #[1.5 2.5]) (even? #[1 2]) (zero? #[0 1 nil]) (log #[1 100] 10)]"), "[#[1 2] #[false true] #[true false nil] #[0.0 2.0]]");
    assert_eq!(eval("(+ #[9223372036854775807] 1)"), "error: integer overflow");
}

#[test]
fn comparisons_broadcast_into_masks() {
    assert_eq!(eval("[(< #[1 2 3] 2) (>= #[1 2] #[2 1]) (= #[1 2 nil] 2) (= #[\"a\" \"b\"] \"a\")]"), "[#[true false false] #[false true] #[false true nil] #[true false]]");
    assert_eq!(eval("[(not #[true false nil]) (not #[1 2])]"), "[#[false true nil] #[false false]]");
    assert_eq!(eval("[(mask-and #[true false] #[true true]) (mask-or #[true false] #[false false]) (mask-and #[true nil] true)]"), "[#[true false] #[true false] #[true nil]]");
    assert_eq!(eval("(< 1 #[2 3] 3)"), "#[true false]");
    assert_eq!(eval("(< #[1 2] \"a\")"), "error: < expects numbers, got \"a\"");
    assert_eq!(eval("[(max #[1 5] 3) (min #[1 5] #[2 2]) (max #[1 5 nil]) (min #[])]"), "[#[3 5] #[1 2] 5 nil]");
}

#[test]
fn arrays_act_as_sequences() {
    assert_eq!(eval("[(count #[1 2 3]) (nth #[1 2 3] 1) (#[1 2 3] 0) (get #[1 2] 5) (first #[7 8]) (last #[1 2])]"), "[3 2 1 nil 7 2]");
    assert_eq!(eval("[(rest #[7 8 9]) (vec #[1 nil]) (map inc #[1 2]) (reduce + #[1 2 3]) (empty? #[]) (nth #[1 nil] 1)]"), "[(8 9) [1 nil] #[2 3] 6 true nil]");
    assert_eq!(eval("[(conj #[1 2] 3) (conj #[1] 2.5) (conj #[1] nil)]"), "[#[1 2 3] #[1.0 2.5] #[1 nil]]");
    assert_eq!(eval("(conj #[1] \"a\")"), "error: array elements must all be numbers, all booleans, or all strings, got 1 and \"a\"");
    assert_eq!(eval("[(count-missing #[1 nil nil]) (count-missing #[])]"), "[2 0]");
    assert_eq!(eval("(sort #[3 1 2])"), "#[1 2 3]");
    assert_eq!(eval("[(take 2 #[1 2 3]) (filter even? #[1 2 3 4]) (map str #[1 2]) (map (lambda (x) [x]) #[1]) (reverse #[1 2])]"), "[#[1 2] #[2 4] #[\"1\" \"2\"] [[1]] #[2 1]]");
}

#[test]
fn statistics_skip_missing_values() {
    assert_eq!(eval("[(sum #[1 2 3]) (sum #[1.5 nil 2]) (sum #[]) (sum [1 2])]"), "[6 3.5 0 3]");
    assert_eq!(eval("[(mean #[1 2 3 nil]) (mean #[]) (median #[3 1 2]) (median #[4 1 3 2]) (median #[])]"), "[2.0 nil 2 2.5 nil]");
    assert_eq!(eval("[(variance #[2 4 4 4 5 5 7 9]) (std #[2 4 4 4 5 5 7 9])]"), "[4.571428571428571 2.138089935299395]");
    assert_eq!(eval("[(variance #[1]) (std #[])]"), "[nil nil]");
    assert_eq!(eval("(cumsum #[1 2 nil 3])"), "#[1 3 nil 6]");
    assert_eq!(eval("[(quantile #[1 2 3 4 5] 0.5) (quantile #[1 2 3 4] 0.5) (quantile #[1 2 3 4 5] 0.25) (quantile #[] 0.5)]"), "[3.0 2.5 2.0 nil]");
    assert_eq!(eval("(sum #[\"a\"])"), "error: sum expects numbers, got \"a\"");
}

#[test]
fn masking_and_gathering_arrays() {
    assert_eq!(eval("[(#[10 20 30] #[true false true]) (#[10 20 30] [0 2]) (#[10 20 30] #[2 0]) (get #[10 20] #[false true])]"), "[#[10 30] #[10 30] #[30 10] #[20]]");
    assert_eq!(eval("[(where #[1 2 3] (> #[1 2 3] 1)) (where [1 2 3] #[true false true]) ([10 20 30] [2 1])]"), "[#[2 3] [1 3] [30 20]]");
    assert_eq!(eval("(#[10 20] #[true])"), "error: mask length 1 does not match array length 2");
    assert_eq!(eval("(#[10 20] [5])"), "error: index 5 is out of bounds for an array of length 2");
    assert_eq!(eval("[(drop-missing #[1 nil 2]) (fill-missing #[1 nil 2] 0) (missing? nil) (missing? 1)]"), "[#[1 2] #[1 0 2] true false]");
}
