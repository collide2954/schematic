# Schematic Guide

Schematic is a small language for working with data. It looks like Scheme,
borrows its data literals and immutable collections from Clojure, and adds
typed arrays and tables for numeric work. This guide assumes no prior
experience with either language. Every example here is also in
`examples/tour.scm`, which you can run with:

```
schematic examples/tour.scm
```

## Running Schematic

Start the interactive prompt (the REPL) with no arguments. Type an expression,
press Enter, and the result prints. If a line is incomplete, the prompt
changes to two spaces and waits for the rest.

```
$ schematic
> (+ 1 2)
3
> (define greeting
    "hello")
greeting
```

Run a file with `schematic file.scm`, evaluate one expression with
`schematic -e "(* 6 7)"`, and leave the REPL with `(exit)` or Ctrl-D. Ask
about any name with `(doc name)`, which prints its entry from the reference,
and find names with `(apropos "chart")`.

If you prefer a window, Schematic Studio (`cargo run --release -p
schematic-studio`) puts an editor, a console, an environment pane, a data
viewer, and a plots pane side by side, each of which can be hidden,
maximized, or detached into a window of its own, say the plots on a second
screen; closing that window puts the pane back, and detached panes come
back the same way at the next start. Ctrl+Enter in the editor runs the form under the cursor and
moves to the next one, so you can step through a script; the console
remembers earlier inputs for the up and down arrows; Ctrl+Space completes
names; and F1 opens this guide and the reference in a Help tab at the name
under the cursor. The editor indents new lines the Lisp way, closes
brackets and quotes as you type, undoes with Ctrl+Z, comments lines with
Ctrl+/, reindents with Ctrl+Shift+F, finds and replaces with Ctrl+F and
Ctrl+H, and grows the selection to the enclosing form with Ctrl+Shift+Up.
An Outline tab lists the file's definitions, the menus at the top hold
every action with its shortcut, and Ctrl+Shift+P opens a command palette
that runs any of them by name; typing a colon and a number there jumps to
that line. The data pane sorts by a column header, filters rows, copies
rows, exports a table as CSV, and plots a column's histogram from its
header; the console finds text, copies any line, and saves its transcript
as Markdown; and a Watch tab keeps expressions evaluated after every run.
Studio remembers its theme, sizes, layout, open files, and watches between
sessions; Ctrl+, opens the Settings tab, Ctrl+= and Ctrl+- resize the code,
a Start page lists recent files and projects, files changed on disk are
reloaded, and plots zoom with the wheel, pan by dragging, and copy as SVG.

A notebook is an ordinary `.scm` file whose cells are separated by `;; %%`
lines; a cell that starts with `;; %% text` holds Markdown in comment lines.
Studio opens such a file as cells: Ctrl+Enter runs the cell under the
cursor, Shift+Enter runs it and moves on, each cell keeps its printed text,
results, errors, and charts beneath it, and those outputs are saved next to
the file in `name.scm.out`. File → New notebook starts one, File → Export
notebook writes it as an HTML page or Markdown, and `schematic run` still
runs the whole file top to bottom, because the markers are comments. File →
Import CSV reads a file into a table named after it, and the Environment
tab offers Describe, Watch, and Copy name for whatever is selected.

The Git tab (Ctrl+8) works on the repository around the open folder through
the `git` on your path: it shows the branch and how far it is ahead of or
behind its upstream, lists staged and unstaged changes, stages, unstages,
or discards a file, commits with a one-line message, pulls, pushes, and
shows recent commits. Clicking a change opens its diff in a tab, and the
Files tab marks changed files with the same letters git uses.

## Expressions

Everything is an expression written in parentheses with the operation first:

```scheme
(+ 1 2)            ; 3
(* 2 (+ 3 4))      ; 14
(max 3 9 4)        ; 9
```

Numbers are either integers or floats. Integer arithmetic never silently
wraps around; overflow is an error. Division always produces a float, and
`quot`, `rem`, and `mod` do integer division.

```scheme
(/ 7 2)            ; 3.5
(quot 7 2)         ; 3
(mod -7 2)         ; 1
(+ 1 2.5)          ; 3.5
```

Text goes in double quotes, and `true`, `false`, and `nil` are themselves.
Only `false` and `nil` count as false in a condition; `0`, `""`, and empty
collections are all true.

## Names and functions

`define` gives a value a name. Writing the name in parentheses with
parameters defines a function.

```scheme
(define tax-rate 0.25)

(define (with-tax price)
  (* price (+ 1 tax-rate)))

(with-tax 100)     ; 125.0
```

Anonymous functions use `lambda`. Functions are values: pass them around,
store them in collections, and return them from other functions.

```scheme
(define double (lambda (x) (* x 2)))
(map double [1 2 3])            ; [2 4 6]

(define (make-adder n)
  (lambda (x) (+ x n)))
((make-adder 10) 5)             ; 15
```

A parameter after `&` collects any remaining arguments into a list:

```scheme
(define (count-args & args) (count args))
(count-args 1 2 3)              ; 3
```

## Local names

`let` binds names for the duration of its body. `let*` lets each binding
see the ones before it.

```scheme
(let ((width 3) (height 4))
  (* width height))             ; 12

(let* ((x 2) (y (* x x)))
  (+ x y))                      ; 6
```

## Making decisions

```scheme
(if (> 5 3) "bigger" "smaller")

(cond ((< 5 0) "negative")
      ((= 5 0) "zero")
      (else "positive"))

(when (> 5 3) (println "yes"))
(unless (> 5 3) (println "never printed"))

(and true 1 "last")             ; "last"
(or nil false 7)                ; 7
```

`and` and `or` stop as soon as the answer is known and return the deciding
value, not just `true` or `false`.

## Taking values apart

A vector or map can stand where a name would, and the names inside it bind
to the matching pieces of the value:

```scheme
(define [low high] [1 9])                    ; low is 1, high is 9
(let (([first & more] [1 2 3]))
  more)                                      ; [2 3]
(define (full-name {:keys [first last]})
  (str first " " last))
(full-name {:first "Ada" :last "Lovelace"})  ; "Ada Lovelace"
```

`match` picks the first clause whose pattern fits and binds names along the
way. Literals must be equal, vectors must have the right length, maps must
have the keys, and `:when` adds a condition:

```scheme
(define (shape-name shape)
  (match shape
    ([:circle r] (str "circle of radius " r))
    ([:rect w h] :when (= w h) (str "square of side " w))
    ([:rect w h] (str w " by " h " rectangle"))
    ({:keys [points]} (str (count points) " points"))
    (else "something else")))

(shape-name [:rect 2 2])                     ; "square of side 2"
(shape-name {:points [1 2 3]})               ; "3 points"
```

## Repetition

Schematic has no loop keyword in the usual sense. A function that calls
itself in tail position runs in constant space, and named `let` gives that
pattern a compact shape:

```scheme
(let loop ((i 0) (total 0))
  (if (= i 5)
      total
      (loop (+ i 1) (+ total i))))        ; 10
```

`loop` and `recur` do the same thing without naming the loop:

```scheme
(loop ((i 0) (acc []))
  (if (= i 3)
      acc
      (recur (+ i 1) (conj acc (* i i)))))  ; [0 1 4]
```

Most of the time you will not write loops at all. `map`, `filter`, and
`reduce` cover the common cases:

```scheme
(map (lambda (x) (* x x)) [1 2 3 4])      ; [1 4 9 16]
(filter even? [1 2 3 4])                  ; [2 4]
(reduce + 0 [1 2 3 4])                    ; 10
```

## Collections

Schematic has four everyday collections. All of them are immutable:
operations return new collections and leave the old ones untouched.

| Literal | Name | Use it for |
|---|---|---|
| `[1 2 3]` | vector | ordered items, fast indexing and appending |
| `{:name "Ada" :age 36}` | map | records and lookups by key |
| `#{1 2 3}` | set | membership |
| `'(1 2 3)` | list | code as data, building from the front |

Keywords such as `:name` are symbolic constants that are mostly used as map
keys. A keyword works as a function that looks itself up, and so do maps,
vectors, and sets:

```scheme
(define ada {:name "Ada" :age 36})
(:name ada)                     ; "Ada"
(ada :age)                      ; 36
(:email ada "none")             ; "none"
([10 20 30] 1)                  ; 20
```

Useful operations:

```scheme
(conj [1 2] 3)                  ; [1 2 3]
(assoc ada :city "London")      ; adds a key
(dissoc ada :age)               ; removes a key
(update ada :age inc)           ; {:name "Ada", :age 37}
(get-in {:a {:b 1}} [:a :b])    ; 1
(count [1 2 3])                 ; 3
(first [1 2 3])                 ; 1
(rest [1 2 3])                  ; (2 3)
(sort [3 1 2])                  ; [1 2 3]
(sort-by :age [{:age 30} {:age 20}])
(group-by even? [1 2 3 4])      ; {false [1 3], true [2 4]}
(frequencies ["a" "b" "a"])     ; {"a" 2, "b" 1}
```

Strings have their own family of functions, all starting with `string-`,
plus `str`, which glues anything together, and `format`, which fills `{}`
placeholders in a template:

```scheme
(str "Total: " 42)                    ; "Total: 42"
(string-split "a,b,c" ",")            ; ["a" "b" "c"]
(string-upcase "hi")                  ; "HI"
(format "{} sold {} units" "Oslo" 12) ; "Oslo sold 12 units"
(format "{:.2}" 3.14159)              ; "3.14"
(format "{:,}" 1234567)               ; "1,234,567"
(format "{:>6}|{:<6}|" "ab" "cd")     ; "    ab|cd    |"
(format "{:.1%}" 0.256)               ; "25.6%"
```

After the colon come, in order, a fill character with an alignment (`<`,
`>`, or `^`), a width, `,` for thousands separators, `.` and a precision, and
a type: `e` for scientific notation, `%` for a percentage, `?` for the
readable form with strings quoted. `{0}` and `{1}` pick arguments by
position, and a brace written twice stands for itself. `printf` prints a formatted string
exactly as given, and `string-pad-left` and `string-pad-right` pad to a width.

Every string function accepts an array of strings in place of a single
string and returns an array, so a text column can be cleaned in one call:

```scheme
(string-upcase #["oslo" "rome"])            ; #["OSLO" "ROME"]
(string-starts-with? #["Ada" "Bob"] "A")    ; #[true false]
(format "{} ({})" #["Ada" "Bob"] #[36 41])  ; #["Ada (36)" "Bob (41)"]
```

Patterns are written as raw strings, `#"..."`, which keep their
backslashes. `re-find`, `re-seq`, `re-matches`, `re-split`, and `re-replace`
take the string first and the pattern second; a pattern with groups returns
the whole match followed by each group, and `re-replace` fills a template
with `$1`, `$2`, and so on, or calls a function on each match. `letter?`,
`digit?`, `whitespace?`, `upper-case?`, and `lower-case?` test every
character of a string, splitting on `""` gives the characters, and
`char-code` and `code-char` convert to and from character codes:

```scheme
(re-find "order 66" #"\d+")                                ; "66"
(re-seq "k=v; a=b" #"(\w+)=(\w+)")                         ; [["k=v" "k" "v"] ["a=b" "a" "b"]]
(re-replace "2024-03-15" #"(\d+)-(\d+)-(\d+)" "$3/$2/$1")  ; "15/03/2024"
(re-split "a, b,c" #",\s*")                                ; ["a" "b" "c"]
(string-join "" (filter letter? (string-split "a1b2" ""))) ; "ab"
(char-code "A")                                            ; 65
```

## Dates

A date is written `#2024-03-15` and stands for a calendar day. Adding a
number moves by days, subtracting two dates counts the days between them, and
comparisons and sorting work as you would expect:

```scheme
(define d #2024-03-15)
(+ d 20)                              ; #2024-04-04
(- #2024-04-01 d)                     ; 17
(< d #2024-04-01)                     ; true
(weekday d)                           ; 5, a Friday
(date-format d "EEEE, MMMM d")        ; "Friday, March 15"
(date "15/03/2024" "dd/MM/yyyy")      ; #2024-03-15
(date-trunc d :month)                 ; #2024-03-01
(add-months #2024-01-31 1)            ; #2024-02-29
(date-range #2024-01-30 #2024-02-02)  ; #[#2024-01-30 #2024-01-31 #2024-02-01 #2024-02-02]
```

A CSV column of ISO dates becomes a date column on its own, and every date
function works on a whole column. Monthly totals are a `date-trunc` away:
add the truncated column with `assoc`, then `aggregate` by it.

## Arrays

Arrays hold many numbers (or booleans, or strings) of one type in a compact
buffer, which makes numeric work fast. Write them with `#[...]` or build one
from any collection with `array`.

```scheme
(define prices #[10 20 30 nil])
(array-type prices)             ; :int
(count-missing prices)          ; 1
```

Arithmetic and comparisons work elementwise over arrays, and a plain number
is applied to every element:

```scheme
(* prices 1.25)                 ; #[12.5 25.0 37.5 nil]
(+ prices #[1 2 3 4])           ; #[11 22 33 nil]
(> prices 15)                   ; #[false true true nil]
```

`nil` marks a missing value. Arithmetic keeps missing values missing, and
statistics skip them:

```scheme
(sum prices)                    ; 60
(mean prices)                   ; 20.0
(median prices)                 ; 20
(std #[2 4 4 4 5 5 7 9])        ; 2.138089935299395
```

A boolean array selects elements, and a vector of positions gathers them:

```scheme
(where prices (> prices 15))    ; #[20 30]
(prices [2 0])                  ; #[30 10]
(fill-missing prices 0)         ; #[10 20 30 0]
(drop-missing prices)           ; #[10 20 30]
```

A few functions look along an array in order, which is what time series
and rankings need. `lag` and `lead` shift values, `diff` takes the change
from the previous element, `rolling` applies a function to each trailing
window, and `rank` numbers elements from smallest to largest:

```scheme
(running-sum prices)            ; #[10 30 60 nil]
(lag prices)                    ; #[nil 10 20 30]
(diff prices)                   ; #[nil 10 10 nil]
(rolling 2 mean prices)         ; #[nil 15.0 25.0 30.0]
(rank #[30 10 20])              ; #[3 1 2]
```

The last rolling mean is `30.0` rather than missing because `mean` skips
missing values inside its window, as it does everywhere else.

Comparisons against arrays use `=` elementwise. To ask whether two values
are the same thing structurally, use `equal?`.

## Tables

A table is a set of equally long named columns. Build one from a map of
columns, from keyword and column pairs, or from a vector of row maps, and
read one from a CSV file with `read-csv`.

```scheme
(define sales
  (table {:city ["Oslo" "Rome" "Oslo" "Rome"]
          :amount [10 20 30 40]
          :year [2020 2020 2021 2021]}))
```

A table prints as an aligned grid. Columns are arrays, so everything above
applies to them:

```scheme
(:amount sales)                 ; #[10 20 30 40]
(mean (:amount sales))          ; 25.0
(columns sales)                 ; [:city :amount :year]
(nrows sales)                   ; 4
(row sales 0)                   ; {:city "Oslo", :amount 10, :year 2020}
```

Filtering takes either a boolean array or a function of a row map. Both
return a new table:

```scheme
(where sales (> (:amount sales) 15))
(where sales (lambda (r) (= (:city r) "Oslo")))
```

Shaping and sorting:

```scheme
(select sales [:city :amount])
(order-by sales :amount :desc)
(assoc sales :with-tax (* (:amount sales) 1.25))
(dissoc sales :year)
(rename sales {:amount :revenue})
(head sales 2)
```

Grouping and summarizing use `aggregate`. The spec maps new column names to
a function and the column it applies to; `[count]` alone counts rows:

```scheme
(aggregate sales :city {:total [sum :amount]
                        :orders [count]})
;; city  total  orders
;; Oslo     40       2
;; Rome     60       2
```

`window` has the same shape as `aggregate` but keeps every row, adding a
column computed within each group. Running totals, shares of a group total,
and ranks within a group all fit:

```scheme
(window sales :city {:running [running-sum :amount]
                     :share (lambda (g) (/ (:amount g) (sum (:amount g))))})
;; city  amount  year  running  share
;; Oslo      10  2020       10                0.25
;; Rome      20  2020       20  0.3333333333333333
;; Oslo      30  2021       40                0.75
;; Rome      40  2021       60  0.6666666666666666
```

`pivot` turns a long table wide, with one column per distinct value of a
column. `unpivot` goes the other way:

```scheme
(pivot sales :city :year :amount)
;; city  2020  2021
;; Oslo    10    30
;; Rome    20    40

(unpivot (pivot sales :city :year :amount) [:2020 :2021] :year :amount)
```

When several rows land in the same cell, pass a function to combine them,
as in `(pivot sales :city :year :amount sum)`.

Rows are added with the same functions that add to other collections.
Columns a row lacks are filled with `nil`, and `distinct` drops repeated
rows:

```scheme
(conj sales {:city "Oslo" :amount 50 :year 2022})
(concat sales more-sales)
(distinct sales :city)
```

Join two tables on a shared column name:

```scheme
(define cities (table {:city ["Oslo" "Rome"] :country ["NO" "IT"]}))
(join sales cities :city)
```

`describe` summarizes every numeric column, and `write-csv` saves a table:

```scheme
(describe sales)
(write-csv sales "sales.csv")
(read-csv "sales.csv")
```

## Charts

A chart is a value too. Printing one in the terminal draws it with text,
Studio shows it in its Plots pane, and `save-plot` writes an SVG file:

```scheme
(bar-chart ["Oslo" "Rome" "Bern"] [42 17 5] {:title "Units"})
; Units
; Oslo ████████████████████████████████████████ 42
; Rome ████████████████ 17
; Bern █████ 5

(define shop (table {:city ["Oslo" "Oslo" "Rome"]
                     :year [2020 2021 2021]
                     :units [12 30 7]
                     :when [#2024-01-01 #2024-02-01 #2024-03-01]}))
(bars shop :city :units {:by :year})   ; grouped bars, one colour per year
(lines shop :when :units)              ; dates along the x axis
(histogram (:units shop) {:bins 2})
(save-plot (points shop :year :units) "units.svg")
```

A chart is built from layers. `bars`, `lines`, `steps`, `areas`, `points`,
`lollipops`, and `pie` take a table with two column keywords, or two plain
collections, and return a chart; given a chart as their first argument they
add a layer to it instead. The `->` macro threads a value through a chain
of calls, so a layered chart reads from top to bottom:

```scheme
(define goals (table {:city ["Oslo" "Rome" "Bern"]
                      :units [12 7 30]
                      :target [15 10 20]
                      :share [0.2 0.1 0.7]}))

(-> (bars goals :city :units)
    (lines goals :city :target {:dash true})
    (points goals :city :share {:axis :right})
    (hline 10 "goal")
    (title "Units" "by city")
    (caption "source: shop"))
```

`title`, `subtitle`, `caption`, `hline`, `vline`, `annotate`, `x-axis`,
`y-axis`, `right-axis`, `legend`, and `palette` decorate a chart, and
`trend` and `smooth` add a fitted line or a rolling mean over the last
layer. Options go in a map at the end of any layer verb: `:by` splits rows
into series and `:facet` into side-by-side panels; `:stack`, `:normalize`,
`:horizontal`, `:sort`, and `:top` shape bars; `:size`, `:shape`, and
`:text` encode more columns in points; `:color`, `:width`, `:opacity`,
`:dash`, and `:labels` style a layer; and `:x-range`, `:y-scale :log`,
`:y-format`, `:hline`, and `:vline` shape the axes. Missing values are
skipped.

```scheme
(bars shop :city :units {:by :year :stack true :labels true})
(lines shop :when :units {:facet :city :y-range [0 40]})
(-> (points shop :year :units {:size :units}) (trend))
(bars shop :city :units {:sort :desc :top 2 :palette :warm})
(density (:units shop))
(box-plot shop :city :units)
(heatmap shop :city :year :units)
(pie shop :city :units {:donut true})
(figure [(bars shop :city :units) (lines shop :when :units)] {:columns 2})
(save-plot (points shop :year :units) "units.svg" {:width 800 :theme :dark})
```

`plot-theme!` sets the palette, grid, and legend every new chart starts
from. At a terminal a printed chart uses fine blocks, braille lines, and
the series colors. In Studio, hovering a chart reads every series at that
point, clicking a legend entry hides its series, clicking a bar filters the
data grid to its category, and dragging a box zooms in.

## When things go wrong

Errors carry a map with `:kind` and `:message`, and `try` catches them:

```scheme
(try (/ 1 "two")
  (catch e (:message e)))        ; "/ expects numbers, got \"two\""

(try (error "no data" {:rows 0})
  (catch e (:data e))
  (finally (println "done")))
```

`raise` throws any value, and `call/ec` gives a function that jumps straight
out of a computation:

```scheme
(call/ec (lambda (return)
  (for-each (lambda (x) (when (> x 2) (return x))) [1 2 3 4])
  'none))                        ; 3
```

## Macros

New syntax can be defined with `define-syntax` and `syntax-rules`. Patterns
match the shape of a form and templates rebuild it; `...` matches any number
of repetitions. Names introduced by a template never clash with yours.

```scheme
(define-syntax unless-zero
  (syntax-rules ()
    ((_ n body ...)
     (if (= n 0) nil (begin body ...)))))

(unless-zero 5 (println "five is not zero"))
```

Quasiquote builds data with holes, which is handy for generating code or
nested structures:

```scheme
(define x 42)
`(x is ~x and doubled ~(* 2 x))   ; (x is 42 and doubled 84)
```

Two macros from the prelude thread a value through a chain of calls. `->`
puts it in as the first argument of each call and `->>` as the last, which
suits `map` and `reduce`; a bare name is called with the value:

```scheme
(-> 1 (+ 2) inc)                    ; (inc (+ 1 2)) = 4
(->> [1 2 3] (map inc) (reduce +))  ; 9
```

## Namespaces

Larger programs split into namespaces, as in Clojure. `ns` switches to a
namespace and creates it if needed, `define` puts names into the current
one, and a name from another namespace is written `prefix/name`. Builtins
live in `schematic.core`, which every namespace sees, and code that never
mentions a namespace runs in `user`.

```scheme
(ns tour.helpers)
(define (shout s) (string-upcase s))

(ns user (:require [tour.helpers :as h :refer [shout]]))
(h/shout "hi")                    ; "HI"
(shout "hi")                      ; "HI", referred by name
(current-ns)                      ; user
```

`(:require [name :as alias :refer [...]])` loads a namespace and gives it a
short alias or brings some of its names in directly; `:refer :all` brings
them all. A namespace called `my.lib` comes from the file `my/lib.scm`,
found on the load path, which starts with the folder of the file being run
and the working directory. `require`, `refer`, and `alias` also work on
their own, with quoted arguments.

## Projects and packages

When a script grows into several files, make it a project:

```
schematic new report
cd report
schematic add text-tools
schematic run
```

`new` creates `report/project.scm` and `report/src/report.scm`. `add` looks
`text-tools` up in the package registry, copies its source into
`report/packages/text-tools`, and writes `project.lock` with the exact
commit and a hash of the files, so whoever checks the project out gets the
same code, and `schematic restore --locked` proves it. `run` loads
`src/report.scm` with `src` and every package on the load path, so the main
file can say `(ns report (:require [text-tools :as t]))`. A package can
depend on packages of its own; `add` brings those along, `project.lock`
records which package needs which, and `schematic packages` lists them all
with their licenses. A project that must stay within certain licenses says
so with `:allowed-licenses ["MIT" "Apache-2.0"]` in `project.scm`, and
`add` refuses anything else. Studio's Project tab does the same from the
window: it creates a project in the open folder, adds and removes packages,
and restores them. The
[packages page](https://collide2954.github.io/schematic/packages.html) lists
what the registry holds and explains how to publish a package of your own.

## Checking types

Schematic is dynamically typed, but it can tell you about some mistakes
before anything runs. `schematic check report.scm` reads a file and reports
what is certain to fail: a call with the wrong number of arguments, a
string passed where only a number will do, a name that is never defined, a
value that is not a function being called. It works out types from
literals, definitions, and the built-in functions, and whatever it cannot
work out stays dynamic, so a clean report means something and every problem
it lists is real. Inside a project, `schematic check` on its own covers
every script under `src`. Studio runs the same check as you type: problems
turn red along with their line numbers, the message appears in the status
bar when the cursor is on one, and F8 jumps to the next.

Annotations are optional, and only the checker reads them. A parameter or
binding written `[name : type]` declares its type, and `: type` after a
signature declares what the function returns:

```scheme
(define (area [r : float]) : float (* r r))
(area 2.0)                        ; 4.0
```

Now `(area "big")` is reported before it runs, and so would be a body that
returns something other than a float. Types are written `int`, `float`,
`number`, `string`, `bool`, `keyword`, `symbol`, `date`, `nil`, `table`,
`plot`, `fn`, and `any`; `[int]` is a vector of integers, `{keyword int}` a
map, `#{string}` a set, `#[float]` an array, `(list int)` a list, `(or int
nil)` either of two, and `(-> int int string)` a function from two integers
to a string.

## Where to go next

The [reference](reference.md) lists every special form and built-in
function with its arguments. `examples/bench.rs` shows how to embed the
interpreter from Rust: create an `Interpreter`, call `eval_str`, and print
the resulting `Value`.
