;; A tour of Schematic. Every expression here also appears in docs/guide.md.

(define (check label expected actual)
  (unless (equal? expected actual)
    (error (str label ": expected " expected " but got " actual))))

;; Help
(check "doc-string" true (string-starts-with? (doc-string "doc") "doc (Help)"))
(check "apropos" true (some (lambda (n) (= n "bar-chart")) (apropos "chart")))

;; Expressions
(check "add" 3 (+ 1 2))
(check "nested" 14 (* 2 (+ 3 4)))
(check "max" 9 (max 3 9 4))
(check "division" 3.5 (/ 7 2))
(check "quot" 3 (quot 7 2))
(check "mod" 1 (mod -7 2))
(check "mixed" 3.5 (+ 1 2.5))

;; Names and functions
(define tax-rate 0.25)
(define (with-tax price) (* price (+ 1 tax-rate)))
(check "with-tax" 125.0 (with-tax 100))

(define double (lambda (x) (* x 2)))
(check "map double" [2 4 6] (map double [1 2 3]))

(define (make-adder n) (lambda (x) (+ x n)))
(check "adder" 15 ((make-adder 10) 5))

(define (count-args & args) (count args))
(check "rest args" 3 (count-args 1 2 3))

;; Local names
(check "let" 12 (let ((width 3) (height 4)) (* width height)))
(check "let*" 6 (let* ((x 2) (y (* x x))) (+ x y)))

;; Decisions
(check "if" "bigger" (if (> 5 3) "bigger" "smaller"))
(check "cond" "positive" (cond ((< 5 0) "negative") ((= 5 0) "zero") (else "positive")))
(check "and" "last" (and true 1 "last"))
(check "or" 7 (or nil false 7))

;; Repetition
(check "named let" 10 (let loop ((i 0) (total 0)) (if (= i 5) total (loop (+ i 1) (+ total i)))))
(check "loop" [0 1 4] (loop ((i 0) (acc [])) (if (= i 3) acc (recur (+ i 1) (conj acc (* i i))))))
(check "map" [1 4 9 16] (map (lambda (x) (* x x)) [1 2 3 4]))
(check "filter" [2 4] (filter even? [1 2 3 4]))
(check "reduce" 10 (reduce + 0 [1 2 3 4]))

;; Collections
(define ada {:name "Ada" :age 36})
(check "keyword lookup" "Ada" (:name ada))
(check "map as function" 36 (ada :age))
(check "default" "none" (:email ada "none"))
(check "vector index" 20 ([10 20 30] 1))
(check "conj" [1 2 3] (conj [1 2] 3))
(check "assoc" {:name "Ada" :age 36 :city "London"} (assoc ada :city "London"))
(check "dissoc" {:name "Ada"} (dissoc ada :age))
(check "update" {:name "Ada" :age 37} (update ada :age inc))
(check "get-in" 1 (get-in {:a {:b 1}} [:a :b]))
(check "count" 3 (count [1 2 3]))
(check "first" 1 (first [1 2 3]))
(check "rest" '(2 3) (rest [1 2 3]))
(check "sort" [1 2 3] (sort [3 1 2]))
(check "sort-by" [{:age 20} {:age 30}] (sort-by :age [{:age 30} {:age 20}]))
(check "group-by" {false [1 3] true [2 4]} (group-by even? [1 2 3 4]))
(check "frequencies" {"a" 2 "b" 1} (frequencies ["a" "b" "a"]))
(check "str" "Total: 42" (str "Total: " 42))
(check "split" ["a" "b" "c"] (string-split "a,b,c" ","))
(check "upcase" "HI" (string-upcase "hi"))
(check "format" "Oslo sold 12 units" (format "{} sold {} units" "Oslo" 12))
(check "format precision" "3.14" (format "{:.2}" 3.14159))
(check "format thousands" "1,234,567" (format "{:,}" 1234567))
(check "format align" "    ab|cd    |" (format "{:>6}|{:<6}|" "ab" "cd"))
(check "format percent" "25.6%" (format "{:.1%}" 0.256))
(check "pad" "007" (string-pad-left "7" 3 "0"))
(check "upcase array" #["OSLO" "ROME"] (string-upcase #["oslo" "rome"]))
(check "starts-with array" #[true false] (string-starts-with? #["Ada" "Bob"] "A"))
(check "format array" #["Ada (36)" "Bob (41)"] (format "{} ({})" #["Ada" "Bob"] #[36 41]))

;; Dates
(define d #2024-03-15)
(check "date add" #2024-04-04 (+ d 20))
(check "date diff" 17 (- #2024-04-01 d))
(check "date compare" true (< d #2024-04-01))
(check "weekday" 5 (weekday d))
(check "date-format" "Friday, March 15" (date-format d "EEEE, MMMM d"))
(check "date parse" #2024-03-15 (date "15/03/2024" "dd/MM/yyyy"))
(check "date-trunc" #2024-03-01 (date-trunc d :month))
(check "add-months" #2024-02-29 (add-months #2024-01-31 1))
(check "date-range" #[#2024-01-30 #2024-01-31 #2024-02-01 #2024-02-02] (date-range #2024-01-30 #2024-02-02))

;; Taking values apart
(define [low high] [1 9])
(check "define pattern" 8 (- high low))
(check "rest pattern" [2 3] (let (([first & more] [1 2 3])) more))
(define (full-name {:keys [first last]}) (str first " " last))
(check "map pattern" "Ada Lovelace" (full-name {:first "Ada" :last "Lovelace"}))
(define (shape-name shape)
  (match shape
    ([:circle r] (str "circle of radius " r))
    ([:rect w h] :when (= w h) (str "square of side " w))
    ([:rect w h] (str w " by " h " rectangle"))
    ({:keys [points]} (str (count points) " points"))
    (else "something else")))
(check "match guard" "square of side 2" (shape-name [:rect 2 2]))
(check "match rect" "2 by 3 rectangle" (shape-name [:rect 2 3]))
(check "match map" "3 points" (shape-name {:points [1 2 3]}))
(check "match else" "something else" (shape-name 42))

;; Charts
(define units (bar-chart ["Oslo" "Rome" "Bern"] [42 17 5] {:title "Units"}))
(check "plot?" true (plot? units))
(check "bar text"
       (str "Units\nOslo " (string-repeat "█" 40) " 42\nRome " (string-repeat "█" 16) " 17\nBern " (string-repeat "█" 5) " 5")
       (str units))
(define shop (table {:city ["Oslo" "Oslo" "Rome"]
                     :year [2020 2021 2021]
                     :units [12 30 7]
                     :when [#2024-01-01 #2024-02-01 #2024-03-01]}))
(check "grouped bars" true (plot? (bar-chart shop :city :units {:by :year})))
(check "line over dates" true (string-contains? (str (line-chart shop :when :units)) "2024-01-01"))
(check "histogram" true (string-starts-with? (str (histogram (:units shop) {:bins 2})) "7–18.5"))
(check "svg" true (string-starts-with? (to-svg (scatter shop :year :units)) "<svg"))
(check "stacked bars" true (plot? (bar-chart shop :city :units {:by :year :stack true :labels true})))
(check "facets" true (string-contains? (str (line-chart shop :when :units {:facet :city :y-range [0 40]})) "── Oslo ──"))
(check "box plot" true (string-contains? (str (box-plot shop :city :units)) "Oslo"))
(check "pie" true (string-contains? (str (pie-chart shop :city :units {:donut true})) "(86%)"))
(check "sideways" true (plot? (bar-chart shop :city :units {:horizontal true :y-format "{:,}" :hline 20})))

;; Arrays
(define prices #[10 20 30 nil])
(check "array-type" :int (array-type prices))
(check "count-missing" 1 (count-missing prices))
(check "scale" #[12.5 25.0 37.5 nil] (* prices 1.25))
(check "add arrays" #[11 22 33 nil] (+ prices #[1 2 3 4]))
(check "mask" #[false true true nil] (> prices 15))
(check "sum" 60 (sum prices))
(check "mean" 20.0 (mean prices))
(check "median" 20 (median prices))
(check "std" 2.138089935299395 (std #[2 4 4 4 5 5 7 9]))
(check "where" #[20 30] (where prices (> prices 15)))
(check "gather" #[30 10] (prices [2 0]))
(check "fill" #[10 20 30 0] (fill-missing prices 0))
(check "drop" #[10 20 30] (drop-missing prices))
(check "running-sum" #[10 30 60 nil] (running-sum prices))
(check "lag" #[nil 10 20 30] (lag prices))
(check "diff" #[nil 10 10 nil] (diff prices))
(check "rolling" #[nil 15.0 25.0 30.0] (rolling 2 mean prices))
(check "rank" #[3 1 2] (rank #[30 10 20]))

;; Tables
(define sales
  (table {:city ["Oslo" "Rome" "Oslo" "Rome"]
          :amount [10 20 30 40]
          :year [2020 2020 2021 2021]}))
(check "column" #[10 20 30 40] (:amount sales))
(check "column mean" 25.0 (mean (:amount sales)))
(check "columns" [:city :amount :year] (columns sales))
(check "nrows" 4 (nrows sales))
(check "row" {:city "Oslo" :amount 10 :year 2020} (row sales 0))
(check "where mask" 3 (nrows (where sales (> (:amount sales) 15))))
(check "where pred" 2 (nrows (where sales (lambda (r) (= (:city r) "Oslo")))))
(check "select" [:city :amount] (columns (select sales [:city :amount])))
(check "order-by" #[40 30 20 10] (:amount (order-by sales :amount :desc)))
(check "assoc column" #[12.5 25.0 37.5 50.0] (:with-tax (assoc sales :with-tax (* (:amount sales) 1.25))))
(check "dissoc column" [:city :amount] (columns (dissoc sales :year)))
(check "rename" [:city :revenue :year] (columns (rename sales {:amount :revenue})))
(check "head" 2 (nrows (head sales 2)))
(define totals (aggregate sales :city {:total [sum :amount] :orders [count]}))
(check "aggregate" [{:city "Oslo" :total 40 :orders 2} {:city "Rome" :total 60 :orders 2}] (rows totals))
(define shares (window sales :city {:running [running-sum :amount]
                                    :share (lambda (g) (/ (:amount g) (sum (:amount g))))}))
(check "window running" #[10 20 40 60] (:running shares))
(check "window share" #[0.25 0.3333333333333333 0.75 0.6666666666666666] (:share shares))
(define wide (pivot sales :city :year :amount))
(check "pivot" [{:city "Oslo" :2020 10 :2021 30} {:city "Rome" :2020 20 :2021 40}] (rows wide))
(check "unpivot" (rows (order-by sales :city)) (rows (unpivot wide [:2020 :2021] :year :amount)))
(check "pivot combine" #[40 60] (:2021 (pivot (conj sales {:city "Oslo" :amount 10 :year 2021} {:city "Rome" :amount 20 :year 2021}) :city :year :amount sum)))
(define more-sales (table {:city ["Rome"] :amount [50] :year [2022]}))
(check "conj row" 5 (nrows (conj sales {:city "Oslo" :amount 50 :year 2022})))
(check "concat tables" 5 (nrows (concat sales more-sales)))
(check "distinct" #["Oslo" "Rome"] (:city (distinct sales :city)))
(define cities (table {:city ["Oslo" "Rome"] :country ["NO" "IT"]}))
(check "join" [:city :amount :year :country] (columns (join sales cities :city)))
(check "describe" 2 (nrows (describe sales)))
(check "csv" (rows sales) (rows (parse-csv (to-csv sales))))

;; Errors
(check "catch" "/ expects numbers, got \"two\"" (try (/ 1 "two") (catch e (:message e))))
(check "error data" {:rows 0} (try (error "no data" {:rows 0}) (catch e (:data e))))
(check "call/ec" 3 (call/ec (lambda (return)
                             (for-each (lambda (x) (when (> x 2) (return x))) [1 2 3 4])
                             'none)))

;; Macros
(define-syntax unless-zero
  (syntax-rules ()
    ((_ n body ...)
     (if (= n 0) nil (begin body ...)))))
(check "macro" "ran" (unless-zero 5 "ran"))
(check "macro zero" nil (unless-zero 0 "ran"))

(define x 42)
(check "quasiquote" '(x is 42 and doubled 84) `(x is ~x and doubled ~(* 2 x)))

(println "tour complete")
