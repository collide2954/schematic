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

## Status

Early development. Working today:

- Reader with Clojure-style literals, comments, and quasiquote
- Persistent vectors, maps, and sets
- Bytecode compiler and VM with proper tail calls, closures, `loop`/`recur`
- `try`/`catch`/`finally`, `raise`, and `call/ec` escape continuations
- Hygienic `syntax-rules` macros
- A standard library of collection, string, numeric, and higher-order functions
- A REPL and file runner

Next up: VM performance work, then typed numeric arrays and tables.

## Running

```
cargo build --release
./target/release/schematic            # REPL
./target/release/schematic file.scm   # run a file
./target/release/schematic -e "(+ 1 2)"
cargo test
```

## License

Apache-2.0
