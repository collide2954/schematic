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
- Namespaces in the style of Clojure: `ns`, `require` with `:as` and
  `:refer`, `refer`, `alias`, qualified names, and files found on a load
  path
- Vector and map destructuring in `define`, `let`, `loop`, and parameter
  lists, plus `match` with guards
- A standard library of collection, string, numeric, and higher-order
  functions, with `format` placeholders, regular expressions written as raw
  `#"..."` strings, character predicates, and text functions that work on
  whole columns
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
- Projects: a `project.scm` manifest, packages vendored into `packages/`
  along with the packages they depend on, pinned by commit and hash in
  `project.lock`, a registry read over HTTPS, SPDX licenses per package
  with an allowed list per project, and `schematic new`, `add`, `remove`,
  `restore --locked`, `packages`, and `run`
- A REPL and file runner, plus `doc` and `apropos` for help at the prompt
- A gradual type checker: `schematic check` and Studio's editor report
  certain mistakes without running anything, inferring types from the code
  and from optional `[name : type]` annotations

Next up: a text round with character functions and regular expressions.

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
Iced. Five resizable panes meet at hairlines, and each can be hidden,
maximized, or detached into its own window (and comes back when that window
closes, or at the next start): a script editor with tabs, line numbers, syntax highlighting,
bracket matching, completion, undo, Lisp-aware indentation, bracket and
quote pairing, comment toggling, reindenting, find and replace, and
selection expansion; a console with highlighting, history, and completion;
an environment table that groups everything you have defined into data,
values, and functions, sharing its pane with Files, Project, and Outline
tabs; a data grid with numbers right-aligned; and a plots pane that walks
through the session's charts, shows values on hover, and saves them as SVG.
File, Edit, View, Run, and Help menus hold every action with its shortcut,
and Ctrl+Shift+P opens a command palette that runs any of them by name. The
data grid sorts by a column, filters rows, copies rows as text, exports a
table as CSV, and shows column statistics and histograms; the console has
find, one-click copy, and transcripts saved and loaded as Markdown; a Watch
tab re-evaluates expressions after every run; the interpreter can be
restarted; and File has a New project dialog.
A Help tab renders the guide and the reference with search, F1 looks up
the name under the cursor, and a Project tab creates a project in the open
folder, adds and removes packages, restores them, and shows every package
in use with its license. The type checker runs
as you type: problems turn red, the status bar explains the one under the
cursor, and F8 jumps to the next. Studio remembers its state between
sessions in a settings file: theme, interface size, code size, word wrap,
window size, pane layout, open and recent files, projects, and watches. A
Settings tab (Ctrl+,) changes them, Ctrl+= and Ctrl+- zoom the code, and a
Start page lists recent files and projects. Files dropped on the window
open, files changed on disk reload (or say so, when they have unsaved
edits), and saving keeps the previous version in a backups folder. Plots
zoom with the wheel, pan by dragging, show a crosshair, copy as SVG, and
line up as thumbnails when there are several. Notebooks are `.scm` files
with `;; %%` cell markers (`;; %% text` for Markdown cells): Studio shows
them as cells that run one at a time, keeps each cell's output beneath it
and in a `.scm.out` sidecar, and exports them as HTML or Markdown, while the
CLI runs them as plain scripts. File → Import CSV loads a file into a
table, and the Environment tab offers Describe, Watch, and Copy name for
the selected binding. A Git tab drives the `git` on your path: branch and
upstream position, staged and unstaged changes with stage, unstage, and
discard, one-line commits, pull, push, recent commits, diffs in a tab, and
change letters next to files in the Files tab. It has light, dark, warm,
and high-contrast themes and bundles the Inter and JetBrains Mono fonts,
so it looks the same everywhere. The interpreter runs on its own thread,
so the window stays responsive and a runaway loop can be stopped.

```
cargo run --release -p schematic-studio               # empty editor
cargo run --release -p schematic-studio script.scm    # open a file
```

In the editor, Ctrl+Enter runs the form under the cursor (or the selection)
and moves to the next one, Ctrl+Shift+Enter runs the whole file, Ctrl+O opens
a script through the native file dialog, Ctrl+S saves, Ctrl+Shift+S saves
under a new name, Ctrl+N opens a new tab, and Ctrl+W closes the current one.
Completion opens as you type and on Ctrl+Space; Tab or Enter accepts, Escape
dismisses. In the console, Enter runs a complete form, Shift+Enter adds a
line, and the up and down arrows walk through earlier inputs. Clicking a name
in the environment pane opens it in the data viewer, and clicking a file in
the Files tab opens it in the editor.

## Reproducible builds

`rust-toolchain.toml` pins the Rust version and `Cargo.lock` pins every
crate, so `cargo build --release --locked` builds the same binary on any
machine of the same platform. To keep source paths out of the binary as
well, build with `RUSTFLAGS=--remap-path-prefix=$PWD=/schematic`. Projects
written in Schematic get the same treatment from `project.lock` and
`schematic restore --locked`.

## Learning more

- [Website](https://collide2954.github.io/schematic/): the guide and reference as web pages
- [Guide](docs/guide.md): a walkthrough for newcomers, from expressions to tables
- [Reference](docs/reference.md): every special form and built-in function
- [Packages](https://collide2954.github.io/schematic/packages.html): what the registry holds and how to publish a package
- [examples/tour.scm](crates/schematic/examples/tour.scm): the guide's examples as a runnable, checked script

## License

Apache-2.0. Schematic Studio bundles the Inter and JetBrains Mono fonts,
which are distributed under the SIL Open Font License; their licence texts
sit next to the font files in `crates/schematic-studio/fonts`.
