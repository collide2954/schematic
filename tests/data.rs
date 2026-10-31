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
fn typed_array_arithmetic_matches_scalar_semantics() {
    assert_eq!(eval("[(+ #[1.5 2.5] #[1 1]) (* #[1 2] 2.5) (- 1.0 #[1 2]) (/ #[1 2] #[2 0]) (/ #[1 2] 4)]"), "[#[2.5 3.5] #[2.5 5.0] #[0.0 -1.0] #[0.5 +inf.0] #[0.25 0.5]]");
    assert_eq!(eval("[(+ #[1 nil 3] #[10 20 nil]) (< #[1.5 nil] 2) (= #[1 2] #[1.0 3.0])]"), "[#[11 nil nil] #[true nil] #[true false]]");
    assert_eq!(eval("(* #[4611686018427387904 1] 2)"), "error: integer overflow");
    assert_eq!(eval("[(sum #[1.5 nil 2.5]) (sum #[1 2 nil]) (mean #[2.0 nil 4.0]) (sum (* #[1 2 3] #[1 2 3]))]"), "[4.0 3 3.0 14]");
    assert_eq!(eval("[(/ 7 2) (/ 1.0 4) (/ 3) (* 1.5 2) (< 1.5 2) (= 2.0 2)]"), "[3.5 0.25 0.3333333333333333 3.0 true true]");
    assert_eq!(eval("(+ #[1 2] #[true false])"), "error: + expects numbers, got true");
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

const PEOPLE: &str = "(define t (table {:name [\"Ada\" \"Bob\"] :age [36 41]}))";

fn with_people(src: &str) -> String {
    eval(&format!("{PEOPLE} {src}"))
}

#[test]
fn tables_are_built_from_columns_or_rows() {
    let grid = "name  age\nAda    36\nBob    41\n(2 rows, 2 columns)";
    assert_eq!(eval("(table {:name [\"Ada\" \"Bob\"] :age [36 41]})"), grid);
    assert_eq!(eval("(table :name [\"Ada\" \"Bob\"] :age [36 41])"), grid);
    assert_eq!(eval("(table [{:name \"Ada\" :age 36} {:name \"Bob\" :age 41}])"), grid);
    assert_eq!(eval("(table {:name #[\"Ada\" \"Bob\"] :age #[36 41]})"), grid);
    assert_eq!(eval("(table {:a [1 2] :b [1]})"), "error: table columns must have the same length: :a has 2 rows but :b has 1");
    assert_eq!(eval("(table {})"), "(0 rows, 0 columns)");
    assert_eq!(eval("(table {:x [1]})"), "x\n1\n(1 row, 1 column)");
    assert_eq!(eval("(table 5)"), "error: table expects a map of columns, a vector of row maps, or keyword and column pairs, got 5");
}

#[test]
fn table_columns_and_rows_are_accessible() {
    assert_eq!(with_people("[(columns t) (nrows t) (count t) (:age t) (col t :name) (t :age)]"), "[[:name :age] 2 2 #[36 41] #[\"Ada\" \"Bob\"] #[36 41]]");
    assert_eq!(with_people("[(row t 1) (rows t) (first t) (map :age t)]"), "[{:name \"Bob\", :age 41} [{:name \"Ada\", :age 36} {:name \"Bob\", :age 41}] {:name \"Ada\", :age 36} [36 41]]");
    assert_eq!(with_people("[(:zzz t) (get t :zzz 0) (table? t) (table? {}) (empty? t) (empty? (table {}))]"), "[nil 0 true false false true]");
    assert_eq!(with_people("(col t :zzz)"), "error: table has no column :zzz");
    assert_eq!(with_people("(row t 5)"), "error: index 5 is out of bounds for a table of length 2");
}

#[test]
fn table_columns_can_be_added_removed_and_selected() {
    assert_eq!(with_people("(define t2 (assoc t :senior (> (:age t) 40))) [(columns t2) (:senior t2) (columns t)]"), "[[:name :age :senior] #[false true] [:name :age]]");
    assert_eq!(with_people("[(:tag (assoc t :tag \"x\")) (:age (assoc t :age [1 2])) (columns (assoc t :age [1 2]))]"), "[#[\"x\" \"x\"] #[1 2] [:name :age]]");
    assert_eq!(with_people("(assoc t :bad [1])"), "error: table columns must have the same length: :name has 2 rows but :bad has 1");
    assert_eq!(with_people("[(columns (dissoc t :age)) (columns (select t [:age :name])) (columns (select t :age)) (columns (rename t {:age :years}))]"), "[[:name] [:age :name] [:age] [:name :years]]");
    assert_eq!(with_people("(select t [:nope])"), "error: table has no column :nope");
    assert_eq!(with_people("[(:age (update t :age inc)) (:age (update t :age (lambda (col) (* col 2))))]"), "[#[37 42] #[72 82]]");
    assert_eq!(with_people("(rows (assoc t :name (map string-upcase (:name t))))"), "[{:name \"ADA\", :age 36} {:name \"BOB\", :age 41}]");
}

#[test]
fn tables_filter_sort_and_slice() {
    assert_eq!(with_people("[(rows (where t (> (:age t) 40))) (nrows (where t #[false false]))]"), "[[{:name \"Bob\", :age 41}] 0]");
    assert_eq!(with_people("(rows (where t (lambda (r) (= (:name r) \"Ada\"))))"), "[{:name \"Ada\", :age 36}]");
    assert_eq!(with_people("(where t 5)"), "error: where expects a mask or a predicate, got 5");
    let t3 = "(define t3 (table {:n [\"b\" \"a\" \"c\"] :v [2 1 2]}))";
    assert_eq!(eval(&format!("{t3} [(:n (order-by t3 :v)) (:n (order-by t3 :v :desc)) (:n (order-by t3 [:v :n] :desc)) (:n (order-by t3 :n))]")), "[#[\"a\" \"b\" \"c\"] #[\"b\" \"c\" \"a\"] #[\"c\" \"b\" \"a\"] #[\"a\" \"b\" \"c\"]]");
    assert_eq!(eval(&format!("{t3} [(:n (head t3 2)) (:n (tail t3 1)) (nrows (head t3 10)) (:v (order-by (table {{:v [nil 2 1]}}) :v))]")), "[#[\"b\" \"a\"] #[\"c\"] 3 #[nil 1 2]]");
    assert_eq!(with_people("(:name (where t (> (:age t) 40)))"), "#[\"Bob\"]");
}

const SALES: &str = "(define s (table {:city [\"Oslo\" \"Rome\" \"Oslo\" \"Rome\" \"Oslo\"] :amount [10 20 30 40 nil] :year [2020 2020 2021 2021 2021]}))";

fn with_sales(src: &str) -> String {
    eval(&format!("{SALES} {src}"))
}

#[test]
fn tables_group_and_aggregate() {
    assert_eq!(with_sales("[(keys (group-by s :city)) (nrows ((group-by s :city) \"Oslo\")) (:amount ((group-by s :city) \"Rome\"))]"), "[[\"Oslo\" \"Rome\"] 3 #[20 40]]");
    assert_eq!(with_sales("(aggregate s :city {:total [sum :amount] :n [count]})"), "city  total  n\nOslo     40  3\nRome     60  2\n(2 rows, 3 columns)");
    assert_eq!(with_sales("(rows (aggregate s {:total [sum :amount] :avg [mean :amount]}))"), "[{:total 100, :avg 25.0}]");
    assert_eq!(
        with_sales("(rows (aggregate s [:city :year] {:n [count]}))"),
        "[{:city \"Oslo\", :year 2020, :n 1} {:city \"Rome\", :year 2020, :n 1} {:city \"Oslo\", :year 2021, :n 2} {:city \"Rome\", :year 2021, :n 1}]"
    );
    assert_eq!(with_sales("[(:biggest (aggregate s :city {:biggest [max :amount]})) (:first-year (aggregate s :city {:first-year [first :year]}))]"), "[#[30 40] #[2020 2020]]");
    assert_eq!(with_sales("(rows (aggregate s :city {:n nrows}))"), "[{:city \"Oslo\", :n 3} {:city \"Rome\", :n 2}]");
    assert_eq!(with_sales("(aggregate s :city {:x [sum :nope]})"), "error: table has no column :nope");
    assert_eq!(with_sales("(aggregate s :city 5)"), "error: aggregate expects a map from new column names to [function column] pairs, got 5");
    assert_eq!(with_sales("(keys (group-by s [:city :year]))"), "[[\"Oslo\" 2020] [\"Rome\" 2020] [\"Oslo\" 2021] [\"Rome\" 2021]]");
}

#[test]
fn tables_drop_duplicate_rows() {
    let setup = "(define d (table {:city [\"Oslo\" \"Rome\" \"Oslo\" \"Oslo\"] :year [2020 2020 2020 2021]}))";
    assert_eq!(eval(&format!("{setup} (rows (distinct d))")), "[{:city \"Oslo\", :year 2020} {:city \"Rome\", :year 2020} {:city \"Oslo\", :year 2021}]");
    assert_eq!(eval(&format!("{setup} [(:city (distinct d :year)) (nrows (distinct d [:city :year])) (nrows (distinct d :city))]")), "[#[\"Oslo\" \"Oslo\"] 3 2]");
    assert_eq!(eval(&format!("{setup} (distinct d :nope)")), "error: table has no column :nope");
    assert_eq!(eval(&format!("{setup} (distinct d 5)")), "error: distinct expects a keyword column name, got 5");
    assert_eq!(eval("(distinct [1 1 2] :x)"), "error: distinct expects a table when given column names, got [1 1 2]");
    assert_eq!(eval("(distinct [1 1 2])"), "[1 2]");
}

#[test]
fn tables_join_on_a_shared_key() {
    let setup = "(define p (table {:id [1 2 3] :name [\"Ada\" \"Bob\" \"Cy\"]})) (define o (table {:id [2 1 2 9] :total [5 7 1 0]}))";
    assert_eq!(eval(&format!("{setup} (rows (join p o :id))")), "[{:id 1, :name \"Ada\", :total 7} {:id 2, :name \"Bob\", :total 5} {:id 2, :name \"Bob\", :total 1}]");
    assert_eq!(eval(&format!("{setup} [(columns (join p o :id)) (nrows (join p o :id :left)) (:total (join p o :id :left))]")), "[[:id :name :total] 4 #[7 5 1 nil]]");
    assert_eq!(eval(&format!("{setup} (join p o :nope)")), "error: table has no column :nope");
    assert_eq!(eval(&format!("{setup} (join p o :id :outer)")), "error: join expects :inner or :left, got :outer");
    assert_eq!(eval("(rows (join (table {:k [1] :v [1]}) (table {:k [1] :v [2]}) :k))"), "[{:k 1, :v 1, :v-right 2}]");
}

#[test]
fn describe_summarizes_numeric_columns() {
    assert_eq!(
        with_sales("(rows (describe s))"),
        "[{:column \"amount\", :count 4, :missing 1, :mean 25.0, :std 12.909944487358056, :min 10, :max 40} {:column \"year\", :count 5, :missing 0, :mean 2020.6, :std 0.5477225575051661, :min 2020, :max 2021}]"
    );
    assert_eq!(eval("(nrows (describe (table {:s [\"a\"]})))"), "0");
}

#[test]
fn csv_text_parses_into_typed_tables_and_back() {
    let csv = "name,age,score,member\\nAda,36,1.5,true\\nBob,,2,false\\n\\\"Cy, Jr\\\",41,,\\n";
    assert_eq!(eval(&format!("(columns (parse-csv \"{csv}\"))")), "[:name :age :score :member]");
    assert_eq!(eval(&format!("(rows (parse-csv \"{csv}\"))")), "[{:name \"Ada\", :age 36, :score 1.5, :member true} {:name \"Bob\", :age nil, :score 2.0, :member false} {:name \"Cy, Jr\", :age 41, :score nil, :member nil}]");
    assert_eq!(eval(&format!("(map array-type (vals (into {{}} (map (lambda (c) [c (col (parse-csv \"{csv}\") c)]) (columns (parse-csv \"{csv}\"))))))")), "[:string :int :float :bool]");
    assert_eq!(eval("(rows (parse-csv \"a;b\\n1;x\" {:sep \";\"}))"), "[{:a 1, :b \"x\"}]");
    assert_eq!(eval("(rows (parse-csv \"a,b\\n\\\"he said \\\"\\\"hi\\\"\\\"\\\",2\"))"), "[{:a \"he said \\\"hi\\\"\", :b 2}]");
    assert_eq!(eval("(nrows (parse-csv \"a,b\"))"), "0");
    assert_eq!(eval("(columns (parse-csv \"\u{FEFF}a,b\n1,2\"))"), "[:a :b]");
    assert_eq!(eval("(to-csv (table {:name [\"Ada\" \"Cy, Jr\"] :age [36 nil]}))"), "\"name,age\\nAda,36\\n\\\"Cy, Jr\\\",\\n\"");
    assert_eq!(eval("(to-csv (table {:q [\"say \\\"hi\\\"\"]}))"), "\"q\\n\\\"say \\\"\\\"hi\\\"\\\"\\\"\\n\"");
}

#[test]
fn csv_files_round_trip() {
    let path = std::env::temp_dir().join(format!("schematic-csv-{}.csv", std::process::id()));
    let path_str = path.to_str().unwrap().replace('\\', "/");
    let src = format!(
        "(define t (table {{:city [\"Oslo\" \"Rome\"] :pop [700000 2800000]}})) (write-csv t \"{path_str}\") (define back (read-csv \"{path_str}\")) [(equal? (rows back) (rows t)) (array-type (:pop back))]"
    );
    assert_eq!(eval(&src), "[true :int]");
    assert!(eval("(read-csv \"definitely/missing.csv\")").starts_with("error: cannot read definitely/missing.csv"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn tables_print_as_grids_with_missing_values_and_truncation() {
    assert_eq!(eval("(table {:n [\"x\" nil] :v [1.5 nil]})"), "n    v\nx    1.5\nnil  nil\n(2 rows, 2 columns)");
    let big = eval("(table {:x (range 25)})");
    assert!(big.starts_with("x\n 0\n 1\n"), "{big}");
    assert!(big.ends_with("19\n... 5 more rows\n(25 rows, 1 column)"), "{big}");
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
