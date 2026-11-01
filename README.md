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
- Vector and map destructuring in `define`, `let`, `loop`, and parameter
  lists, plus `match` with guards
- A standard library of collection, string, numeric, and higher-order
  functions, with `format` placeholders and text functions that work on whole
  columns
- Dates with a `#2024-03-15` literal, day arithmetic, parsing and formatting
  patterns, truncation to week, month, quarter, or year, and date columns
  inferred from CSV
- Typed arrays with missing values, broadcasting arithmetic, statistics, and
  ordered helpers such as `lag`, `diff`, `rank`, `rolling`, and `running-sum`
- Tables with `select`, `where`, `distinct`, `order-by`, `group-by`,
  `aggregate`, `window`, `pivot`, `unpivot`, `join`, `describe`, row
  appending, and CSV reading and writing
- Charts: bar (grouped, stacked, or sideways), line, area, scatter,
  histogram, box plot, and pie as values that print as text, save as SVG,
  and show in Studio's Plots pane, with facets, log scales, formatted
  axes, reference lines, and value labels
- A REPL and file runner

Next up: more performance work.

## Running

The repository is a Cargo workspace: `crates/schematic` is the language and
its command-line runner, `crates/schematic-studio` is the desktop workbench.

```
cargo build --release -p schematic
./target/release/schematic            # REPL
./target/release/schematic file.scm   # run a file
./target/release/schematic -e "(+ 1 2)"
cargo test
```

## Schematic Studio

Schematic Studio is a desktop workbench in the spirit of RStudio, built with
Iced. Five resizable panes meet at hairlines: a script editor with line
numbers, syntax highlighting, and bracket matching; a console with history;
an environment table that groups everything you have defined into data,
values, and functions; a data grid with numbers right-aligned; and a plots
pane that draws charts and saves them as SVG. It has its own light and dark
themes and bundles
the Inter and JetBrains Mono fonts, so it looks the same everywhere. The
interpreter runs on its own thread, so the window stays responsive and a
runaway loop can be stopped.

```
cargo run --release -p schematic-studio               # empty editor
cargo run --release -p schematic-studio script.scm    # open a file
```

In the editor, Ctrl+Enter runs the form under the cursor (or the selection)
and moves to the next one, Ctrl+Shift+Enter runs the whole file, Ctrl+O opens
a script through the native file dialog, Ctrl+S saves, and Ctrl+Shift+S saves
under a new name. In the console, Enter runs a complete form, Shift+Enter adds
a line, and the up and down arrows walk through earlier inputs. Clicking a
name in the environment pane opens it in the data viewer.

## Learning more

- [Website](https://collide2954.github.io/schematic/): the guide and reference as web pages
- [Guide](docs/guide.md): a walkthrough for newcomers, from expressions to tables
- [Reference](docs/reference.md): every special form and built-in function
- [examples/tour.scm](crates/schematic/examples/tour.scm): the guide's examples as a runnable, checked script

## License

Apache-2.0. Schematic Studio bundles the Inter and JetBrains Mono fonts,
which are distributed under the SIL Open Font License; their licence texts
sit next to the font files in `crates/schematic-studio/fonts`.
