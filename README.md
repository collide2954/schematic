# Schematic

A small, fast Lisp for working with data, written in Rust.

Schematic keeps the shape of Scheme (`define`, `lambda`, `let`, proper tail
calls) and borrows Clojure's data literals and immutable collections. It is
meant to feel like a Scheme while staying approachable for people who have
never used one.

```scheme
(define (fact n)
  (if (< n 2) 1 (* n (fact (- n 1)))))

(define (counter)
  (let ((n 0))
    (lambda () (set! n (+ n 1)) n)))

(define point {:x 1, :y 2})

(let loop ((i 0) (acc 0))
  (if (= i 10) acc (loop (+ i 1) (+ acc i))))
```

Data work uses typed arrays and column-oriented tables. Arithmetic and
comparisons broadcast over arrays, and `nil` marks missing values.

```scheme
(define sales (read-csv "sales.csv"))

(define big (where sales (> (:amount sales) 100)))

(aggregate sales :city {:total [sum :amount]
                        :orders [count]})

(mean (:amount sales))        ; missing values are skipped
(* (:amount sales) 1.25)      ; elementwise
```

## Status

Early development. Working today:

- Reader with Clojure-style literals, comments, and quasiquote
- Persistent vectors, maps, and sets
- Bytecode compiler and VM with proper tail calls, closures, `loop`/`recur`
- `try`/`catch`/`finally`, `raise`, and `call/ec` escape continuations
- Hygienic `syntax-rules` macros
- A standard library of collection, string, numeric, and higher-order functions
- Typed arrays with missing values, broadcasting arithmetic, statistics, and
  ordered helpers such as `lag`, `diff`, `rank`, `rolling`, and `running-sum`
- Tables with `select`, `where`, `distinct`, `order-by`, `group-by`,
  `aggregate`, `window`, `pivot`, `unpivot`, `join`, `describe`, row
  appending, and CSV reading and writing
- A REPL and file runner

Next up: string formatting, plotting-friendly output, and more performance work.

## Running

```
cargo build --release
./target/release/schematic            # REPL
./target/release/schematic file.scm   # run a file
./target/release/schematic -e "(+ 1 2)"
cargo test
```

## Learning more

- [Guide](docs/guide.md): a walkthrough for newcomers, from expressions to tables
- [Reference](docs/reference.md): every special form and built-in function
- [examples/tour.scm](examples/tour.scm): the guide's examples as a runnable, checked script

## License

Apache-2.0
