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
    assert_eq!(eval("(array [1 \"a\"])"), "error: array elements must all be numbers, all booleans, all strings, or all dates, got 1 and \"a\"");
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
    assert_eq!(eval("(conj #[1] \"a\")"), "error: array elements must all be numbers, all booleans, all strings, or all dates, got 1 and \"a\"");
    assert_eq!(eval("[(count-missing #[1 nil nil]) (count-missing #[])]"), "[2 0]");
    assert_eq!(eval("(sort #[3 1 2])"), "#[1 2 3]");
    assert_eq!(eval("[(take 2 #[1 2 3]) (filter even? #[1 2 3 4]) (map str #[1 2]) (map (lambda (x) [x]) #[1]) (reverse #[1 2])]"), "[#[1 2] #[2 4] #[\"1\" \"2\"] [[1]] #[2 1]]");
}

const PEOPLE: &str = "(define t (table {:name [\"Ada\" \"Bob\"] :age [36 41]}))";

fn with_people(src: &str) -> String {
    eval(&format!("{PEOPLE} {src}"))
}

#[test]
fn date_columns_are_inferred_sorted_and_filtered() {
    let t = "(define t (parse-csv \"when,amount\\n2024-03-15,10\\n2024-01-02,5\\n,7\"))";
    assert_eq!(
        eval(&format!("{t} [(array-type (:when t)) (rows (order-by t :when)) (nrows (where t (> (:when t) #2024-02-01))) (min (:when t))]")),
        "[:date [{:when nil, :amount 7} {:when #2024-01-02, :amount 5} {:when #2024-03-15, :amount 10}] 1 #2024-01-02]"
    );
    assert_eq!(eval(&format!("{t} (to-csv (head t 1))")), "\"when,amount\\n2024-03-15,10\\n\"");
    assert_eq!(
        eval(&format!("{t} (rows (aggregate (assoc t :month (date-trunc (:when t) :month)) :month {{:total [sum :amount]}}))")),
        "[{:month #2024-03-01, :total 10} {:month #2024-01-01, :total 5} {:month nil, :total 7}]"
    );
}

#[test]
fn charts_read_table_columns_and_groups() {
    let ages = format!("Ada {} 36\nBob {} 41", "█".repeat(35), "█".repeat(40));
    assert_eq!(with_people("(str (bar-chart t :name :age))"), format!("{ages:?}"));
    let sales = "(define s (table {:city [\"Oslo\" \"Oslo\" \"Rome\"] :year [2020 2021 2021] :units [12 30 7] :when [#2024-01-01 #2024-02-01 #2024-03-01]}))";
    let grouped = format!("Oslo 2020 {} 12\nOslo 2021 {} 30\nRome 2021 {} 7", "█".repeat(16), "█".repeat(40), "█".repeat(9));
    assert_eq!(eval(&format!("{sales} (str (bar-chart s :city :units {{:by :year}}))")), format!("{grouped:?}"));
    let lines = eval(&format!("{sales} (str (line-chart s :when :units {{:by :city}}))"));
    assert!(lines.contains('•') && lines.contains('○') && lines.contains("• Oslo") && lines.contains("○ Rome"), "{lines}");
    assert!(lines.contains("2024-01-01") && lines.contains("2024-03-01"), "{lines}");
    assert!(eval(&format!("{sales} (line-chart s :nope :units)")).contains("nope"));
    assert_eq!(eval(&format!("{sales} (str (histogram s :units {{:bins 2}}))")), format!("{:?}", format!("7–18.5  {} 2\n18.5–30 {} 1", "█".repeat(40), "█".repeat(20))));
}

#[test]
fn rows_destructure_in_lambda_parameters() {
    assert_eq!(with_people("(vec (map (lambda ({:keys [name age]}) (str name \":\" age)) t))"), "[\"Ada:36\" \"Bob:41\"]");
    assert_eq!(with_people("(let (([{:name first-name} & _] (rows t))) first-name)"), "\"Ada\"");
}

#[test]
fn text_functions_work_on_table_columns() {
    assert_eq!(
        with_people("(rows (assoc t :label (format \"{} ({})\" (:name t) (:age t))))"),
        "[{:name \"Ada\", :age 36, :label \"Ada (36)\"} {:name \"Bob\", :age 41, :label \"Bob (41)\"}]"
    );
    assert_eq!(with_people("(rows (where t (string-starts-with? (:name t) \"A\")))"), "[{:name \"Ada\", :age 36}]");
    assert_eq!(with_people("(:name (update t :name string-upcase))"), "#[\"ADA\" \"BOB\"]");
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
fn rows_append_to_tables_with_conj_into_and_concat() {
    assert_eq!(with_people("(rows (conj t {:name \"Cy\" :age 29}))"), "[{:name \"Ada\", :age 36} {:name \"Bob\", :age 41} {:name \"Cy\", :age 29}]");
    assert_eq!(with_people("[(row (conj t {:name \"Cy\"}) 2) (nrows (conj t {:name \"Cy\"} {:name \"Di\"})) (nrows t)]"), "[{:name \"Cy\", :age nil} 4 2]");
    assert_eq!(with_people("(define t2 (conj t {:name \"Cy\" :city \"Oslo\"})) [(columns t2) (:city t2) (:age t2)]"), "[[:name :age :city] #[nil nil \"Oslo\"] #[36 41 nil]]");
    assert_eq!(with_people("(:age (conj t {:name \"Cy\" :age 29.5}))"), "#[36.0 41.0 29.5]");
    assert_eq!(with_people("(conj t 5)"), "error: conj onto a table expects row maps, got 5");
    assert_eq!(with_people("(conj t {:age \"old\"})"), "error: array elements must all be numbers, all booleans, all strings, or all dates, got 36 and \"old\"");
    assert_eq!(with_people("[(nrows (into t [{:name \"Cy\"} {:name \"Di\"}])) (rows (reduce conj (table {}) [{:a 1} {:a 2 :b 3}]))]"), "[4 [{:a 1, :b nil} {:a 2, :b 3}]]");
    assert_eq!(with_people("(rows (concat t (table {:name [\"Cy\"] :age [29]})))"), "[{:name \"Ada\", :age 36} {:name \"Bob\", :age 41} {:name \"Cy\", :age 29}]");
    assert_eq!(with_people("(define t3 (concat t (table {:city [\"Oslo\"]}) [{:name \"Di\"}])) [(columns t3) (nrows t3) (:age t3) (:city t3)]"), "[[:name :age :city] 4 #[36 41 nil nil] #[nil nil \"Oslo\" nil]]");
    assert_eq!(with_people("[(nrows (concat t)) (count (concat [] t)) (nrows (concat (table {}) t))]"), "[2 2 2]");
    assert_eq!(with_people("(concat t 5)"), "error: concat expects tables or collections of row maps, got 5");
}

#[test]
fn tables_pivot_between_long_and_wide() {
    let long = "(define long (table {:city [\"Oslo\" \"Rome\" \"Oslo\"] :year [2020 2020 2021] :amount [10 20 30]}))";
    assert_eq!(eval(&format!("{long} (pivot long :city :year :amount)")), "city  2020  2021\nOslo    10    30\nRome    20   nil\n(2 rows, 3 columns)");
    assert_eq!(eval(&format!("{long} [(columns (pivot long :city :year :amount)) (:2021 (pivot long :city :year :amount))]")), "[[:city :2020 :2021] #[30 nil]]");
    assert_eq!(eval(&format!("{long} (rows (pivot long :year :city :amount))")), "[{:year 2020, :Oslo 10, :Rome 20} {:year 2021, :Oslo 30, :Rome nil}]");
    assert_eq!(eval(&format!("{long} (columns (pivot long [:city :year] :year :amount))")), "[:city :year :2020 :2021]");
    assert_eq!(with_sales("(pivot s :city :year :amount)"), "error: pivot found 2 values for city \"Oslo\" and year 2021; pass a function to combine them");
    assert_eq!(with_sales("[(rows (pivot s :city :year :amount sum)) (:2021 (pivot s :city :year :amount count))]"), "[[{:city \"Oslo\", :2020 10, :2021 30} {:city \"Rome\", :2020 20, :2021 40}] #[2 1]]");
    assert_eq!(eval(&format!("{long} (pivot long :city :nope :amount)")), "error: table has no column :nope");
    assert_eq!(eval(&format!("{long} (pivot long :city :year 5)")), "error: pivot expects a keyword column name, got 5");
    let wide = format!("{long} (define wide (pivot long :city :year :amount))");
    assert_eq!(
        eval(&format!("{wide} (rows (unpivot wide [:2020 :2021] :year :amount))")),
        "[{:city \"Oslo\", :year 2020, :amount 10} {:city \"Oslo\", :year 2021, :amount 30} {:city \"Rome\", :year 2020, :amount 20} {:city \"Rome\", :year 2021, :amount nil}]"
    );
    assert_eq!(eval(&format!("{wide} (equal? (rows (where (unpivot wide [:2020 :2021] :year :amount) (lambda (r) (:amount r)))) (rows (order-by long :city)))")), "true");
    assert_eq!(eval("(rows (unpivot (table {:id [1 2] :a [1 3] :b [2 4]}) [:a :b] :key :value))"), "[{:id 1, :key \"a\", :value 1} {:id 1, :key \"b\", :value 2} {:id 2, :key \"a\", :value 3} {:id 2, :key \"b\", :value 4}]");
    assert_eq!(eval(&format!("{wide} (unpivot wide [:nope] :year :amount)")), "error: table has no column :nope");
}

#[test]
fn window_adds_columns_computed_per_group() {
    assert_eq!(with_sales("[(:running (window s :city {:running [running-sum :amount]})) (:prev (window s :city {:prev [lag :amount]}))]"), "[#[10 20 40 60 nil] #[nil nil 10 20 30]]");
    assert_eq!(with_sales("(:share (window s :city {:share (lambda (g) (/ (:amount g) (sum (:amount g))))}))"), "#[0.25 0.3333333333333333 0.75 0.6666666666666666 nil]");
    assert_eq!(with_sales("[(:n (window s :city {:n [count]})) (:rank (window s :city {:rank [rank :amount]})) (:total (window s {:total [sum :amount]}))]"), "[#[3 2 3 2 3] #[1 1 2 2 nil] #[100 100 100 100 100]]");
    assert_eq!(with_sales("[(columns (window s :city {:a [count] :b [sum :amount]})) (:amount (window s :city {:amount [running-sum :amount]})) (columns s)]"), "[[:city :amount :year :a :b] #[10 20 40 60 nil] [:city :amount :year]]");
    assert_eq!(with_sales("(:label (window s :city {:label [(lambda (g) (str (first (:city g)) \"!\"))]}))"), "#[\"Oslo!\" \"Rome!\" \"Oslo!\" \"Rome!\" \"Oslo!\"]");
    assert_eq!(with_sales("(window s :city {:bad (lambda (g) [1 2])})"), "error: window function for :bad returned 2 values for a group of 3 rows");
    assert_eq!(with_sales("(window s :city {:x [sum :nope]})"), "error: table has no column :nope");
    assert_eq!(with_sales("(window s :city 5)"), "error: window expects a map from new column names to [function column] pairs, got 5");
    assert_eq!(with_sales("(window s :city {:x [1 2 3]})"), "error: window expects [function] or [function column], got [1 2 3]");
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
    assert_eq!(eval("[(running-sum #[1 2 nil 3]) (running-max #[1 3 nil 2 5]) (running-min #[3 1 nil 2 0]) (running-sum [1 2])]"), "[#[1 3 nil 6] #[1 3 nil 3 5] #[3 1 nil 1 0] [1 3]]");
    assert_eq!(eval("(running-max [1 \"a\"])"), "error: running-max expects numbers, got \"a\"");
    assert_eq!(eval("[(quantile #[1 2 3 4 5] 0.5) (quantile #[1 2 3 4] 0.5) (quantile #[1 2 3 4 5] 0.25) (quantile #[] 0.5)]"), "[3.0 2.5 2.0 nil]");
    assert_eq!(eval("(sum #[\"a\"])"), "error: sum expects numbers, got \"a\"");
}

#[test]
fn series_helpers_shift_rank_and_roll() {
    assert_eq!(eval("[(rank #[30 10 20]) (rank #[10 20 20 30 nil]) (rank #[\"b\" \"a\"]) (rank #[]) (rank [2 1])]"), "[#[3 1 2] #[1 2 2 4 nil] #[2 1] #[] [2 1]]");
    assert_eq!(eval("(rank [1 \"a\"])"), "error: rank cannot compare 1 and \"a\"");
    assert_eq!(eval("[(lag #[1 2 3]) (lag #[1 2 3] 2) (lead #[1 2 3]) (lead [1 2 3] 5) (lag #[1 2] 0)]"), "[#[nil 1 2] #[nil nil 1] #[2 3 nil] [nil nil nil] #[1 2]]");
    assert_eq!(eval("(lag #[1] -1)"), "error: lag expects a non-negative count, got -1");
    assert_eq!(eval("[(diff #[1 4 9 nil 16]) (diff #[1.5 2]) (diff #[])]"), "[#[nil 3 5 nil nil] #[nil 0.5] #[]]");
    assert_eq!(eval("(diff #[true false])"), "error: - expects numbers, got false");
    assert_eq!(eval("[(rolling 2 sum #[1 2 3 4]) (rolling 3 mean #[1 2 3 4]) (rolling 2 sum #[1 nil 3]) (rolling 2 count [1 2 3])]"), "[#[nil 3 5 7] #[nil nil 2.0 3.0] #[nil 1 3] [nil 2 2]]");
    assert_eq!(eval("(rolling 0 sum #[1])"), "error: rolling expects a positive window size, got 0");
}

#[test]
fn masking_and_gathering_arrays() {
    assert_eq!(eval("[(#[10 20 30] #[true false true]) (#[10 20 30] [0 2]) (#[10 20 30] #[2 0]) (get #[10 20] #[false true])]"), "[#[10 30] #[10 30] #[30 10] #[20]]");
    assert_eq!(eval("[(where #[1 2 3] (> #[1 2 3] 1)) (where [1 2 3] #[true false true]) ([10 20 30] [2 1])]"), "[#[2 3] [1 3] [30 20]]");
    assert_eq!(eval("(#[10 20] #[true])"), "error: mask length 1 does not match array length 2");
    assert_eq!(eval("(#[10 20] [5])"), "error: index 5 is out of bounds for an array of length 2");
    assert_eq!(eval("[(drop-missing #[1 nil 2]) (fill-missing #[1 nil 2] 0) (missing? nil) (missing? 1)]"), "[#[1 2] #[1 0 2] true false]");
}
