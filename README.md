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

Early development. The reader, persistent vectors, maps, and sets, and the
core of the bytecode compiler and virtual machine are in place. The standard
library, `syntax-rules` macros, error handling, and the REPL are in progress.

## Building

```
cargo build --release
cargo test
```

## License

Apache-2.0
