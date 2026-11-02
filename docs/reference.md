# Schematic Reference

Arguments in square brackets are optional. `coll` means any collection:
list, vector, map, set, array, table, or string where it makes sense.
Functions that take a function argument accept anything callable, including
keywords, maps, vectors, sets, and arrays.

## Syntax

| Form | Reads as |
|---|---|
| `42`, `-7`, `1_000_000` | integer |
| `2.5`, `1e3`, `+inf.0`, `-inf.0`, `+nan.0` | float |
| `"text\n"` | string with `\n`, `\t`, `\r`, `\0`, `\"`, `\\` escapes |
| `#"\d+"` | raw string: backslashes stay as written, and `\"` keeps both characters without ending the string |
| `true`, `false`, `nil` | booleans and nil |
| `:name` | keyword |
| `name`, `set!`, `string->number` | symbol |
| `(a b c)` | list |
| `[a b c]` | vector |
| `{:k v, :k2 v2}` | map (commas are whitespace) |
| `#{a b}` | set |
| `#[1 2 nil]` | array |
| `#2024-03-15` | date |
| `'x`, `(quote x)` | quoted datum |
| `` `(a ~b ~@c) `` | quasiquote with unquote and unquote-splicing |
| `; text` | line comment |
| `#_ form` | comment out one form |
| `#\| text \|#` | block comment, nestable |

## Special forms

| Form | Meaning |
|---|---|
| `(define name value)` | bind a global, or a local at the start of a body |
| `(define (name params...) body...)` | define a function; `& rest` collects extra arguments |
| `(lambda (params...) body...)` | anonymous function; `(lambda args ...)` takes any number |
| `(set! name value)` | assign an existing variable; returns the value |
| `(if test then [else])` | conditional; missing else gives `nil` |
| `(cond (test body...) ... (else body...))` | first clause whose test is true; a bare `(test)` returns the test value |
| `(when test body...)`, `(unless test body...)` | one-armed conditionals returning the last body value or `nil` |
| `(and a b ...)`, `(or a b ...)` | short-circuit; return the deciding value; `(and)` is `true`, `(or)` is `false` |
| `(begin a b ...)` | evaluate in order, return the last |
| `(let ((n v) ...) body...)` | parallel local bindings |
| `(let* ((n v) ...) body...)` | sequential local bindings |
| `(let name ((n v) ...) body...)` | named let: `name` is a function of the bindings for looping |
| `(loop ((n v) ...) body...)`, `(recur v...)` | loop with in-place rebinding; `recur` must be in tail position and also restarts a plain function |
| `(try body... (catch e handler...) (finally cleanup...))` | catch raised values; `finally` always runs |
| `(call/ec (lambda (k) body...))` | escape continuation: calling `k` returns from `call/ec` immediately |
| `(quote x)`, `(quasiquote x)` | literal data, with `unquote` and `unquote-splicing` inside quasiquote |
| `(define [a b & rest] value)`, `(define {:keys [x y]} value)` | destructure a value into several names; patterns also stand for names in `let`, `let*`, `loop`, named `let`, and parameter lists |
| `(match value (pattern [:when test] body...) ... (else body...))` | run the first clause whose pattern fits the value; no fitting clause is an error |
| `(define-syntax name (syntax-rules (literals...) (pattern template)...))` | hygienic macro; `_` matches anything, `...` repeats |
| `(-> x (f a...) g ...)`, `(->> x (f a...) g ...)` | thread `x` through the calls, as the first argument or as the last; a bare `g` is called as `(g x)` |

Proper tail calls are guaranteed, so recursion in tail position never grows
the stack. Non-tail recursion is limited to one million frames.

## Patterns

A pattern can stand wherever a name is bound. `[a b & rest :as all]` takes
elements by position from a vector, list, array, or string, collects what is
left into `rest`, and names the whole value `all`; when binding, a missing
element is `nil`. `{:name n :address {:street s}}` takes map entries by key,
`{:keys [x y]}` takes `:x` and `:y` into `x` and `y`, `:or {y 10}` supplies
defaults, and `:as m` names the whole map. `_` skips an element, and
patterns nest.

In `match` a pattern also tests the value: a vector pattern needs exactly
that many elements, or at least that many with `&`; a map pattern needs each
key to be present unless `:or` gives it a default; a number, string,
keyword, date, `nil`, `true`, `false`, or quoted symbol must be equal to the
value. `:when test` adds a condition that can use the names the pattern
bound, and `else` or `_` catches everything.

## Numbers

| Function | Description |
|---|---|
| `(+ x...)`, `(- x y...)`, `(* x...)` | arithmetic; integers stay integers, overflow is an error |
| `(/ x y...)` | division, always a float; `(/ x)` is the reciprocal |
| `(quot a b)`, `(rem a b)`, `(mod a b)` | integer division, truncating remainder, flooring modulus |
| `(< a b...)`, `(<= a b...)`, `(> a b...)`, `(>= a b...)` | numeric comparison, chainable |
| `(= a b...)` | equality: structural, with `1` equal to `1.0`; elementwise on arrays |
| `(not x)` | logical negation; elementwise on arrays |
| `(max x...)`, `(min x...)` | extremes; a single array argument reduces it |
| `(abs x)`, `(inc x)`, `(dec x)` | absolute value, plus one, minus one |
| `(even? n)`, `(odd? n)`, `(zero? x)`, `(pos? x)`, `(neg? x)` | predicates |
| `(floor x)`, `(ceil x)`, `(round x)`, `(truncate x)` | to integer; `round` goes to the nearest even on ties |
| `(int x)`, `(float x)` | conversions |
| `(sqrt x)`, `(exp x)`, `(log x [base])`, `(pow a b)` | math; `pow` of two integers is an integer |
| `(mask-and a b)`, `(mask-or a b)` | elementwise boolean combination of arrays |

All of these broadcast over arrays: an array operand applies the function to
every element, a plain number is repeated, and missing values stay missing.

## Equality and types

| Function | Description |
|---|---|
| `(equal? a b)` | structural equality, never elementwise |
| `(compare a b)` | `-1`, `0`, or `1`; nil sorts first, then numbers, strings, keywords, vectors |
| `(nil? x)`, `(missing? x)` | nil check |
| `(boolean? x)`, `(integer? x)`, `(float? x)`, `(number? x)`, `(string? x)` | type predicates |
| `(symbol? x)`, `(keyword? x)`, `(procedure? x)` | more type predicates |
| `(list? x)`, `(vector? x)`, `(map? x)`, `(set? x)`, `(array? x)`, `(table? x)`, `(coll? x)` | collection predicates |

## Collections

| Function | Description |
|---|---|
| `(list x...)`, `(vector x...)`, `(hash-map k v...)`, `(hash-set x...)` | constructors |
| `(vec coll)`, `(set coll)`, `(array coll)` | conversions |
| `(range end)`, `(range start end [step])` | vector of numbers |
| `(first coll)`, `(second coll)`, `(last coll)`, `(rest coll)` | access; `rest` returns a list |
| `(nth coll i [default])` | positional access, error when out of range without a default |
| `(get coll key [default])` | lookup by key or index, `nil` when absent |
| `(count coll)`, `(empty? coll)` | size |
| `(cons x list)` | prepend |
| `(conj coll x...)` | add in the natural place: end of a vector or array, front of a list, into a set or map, a row map onto a table |
| `(assoc coll k v...)` | set keys in a map, indices in a vector, or columns in a table |
| `(dissoc coll k...)`, `(disj set x...)` | remove keys or members |
| `(update coll k f args...)` | replace the value at `k` with `(f value args...)` |
| `(get-in coll [k...] [default])`, `(assoc-in coll [k...] v)`, `(update-in coll [k...] f args...)` | nested variants |
| `(keys map)`, `(vals map)`, `(find map k)` | map parts; `find` returns `[k v]` or `nil` |
| `(contains? coll k)` | key, member, or index presence |
| `(merge map...)`, `(select-keys map [k...])`, `(zipmap keys vals)` | map building |
| `(subvec v start [end])` | vector slice |
| `(take n coll)`, `(drop n coll)`, `(take-while f coll)`, `(drop-while f coll)` | prefixes and suffixes |
| `(concat coll...)`, `(into to from)`, `(interleave coll...)`, `(flatten coll)` | combining; `concat` starting from a table stacks tables or row maps |
| `(distinct coll)`, `(frequencies coll)`, `(partition n [step] coll)`, `(reverse coll)` | reshaping |

Results follow the input: a list in gives a list out, an array gives an array
when the elements allow it, and anything else gives a vector.

## Higher-order functions

| Function | Description |
|---|---|
| `(map f coll...)` | apply `f` to each element, or to corresponding elements of several collections |
| `(filter f coll)`, `(remove f coll)` | keep or drop elements where `f` is true |
| `(reduce f [init] coll)` | fold from the left |
| `(for-each f coll)` | call `f` for its effect, return `nil` |
| `(some f coll)`, `(every? f coll)` | first true result, or whether all are true |
| `(sort [cmp] coll)`, `(sort-by key [cmp] coll)` | stable sort; `cmp` returns a negative number or `true` when its first argument comes first |
| `(group-by f coll)` | map from result of `f` to vector of elements |
| `(apply f args... coll)` | call with the collection spread as arguments |
| `(identity x)`, `(constantly x)`, `(complement f)` | trivial functions |
| `(partial f args...)`, `(comp f...)`, `(juxt f...)` | function builders |

## Strings

| Function | Description |
|---|---|
| `(str x...)` | concatenate display forms; `nil` contributes nothing |
| `(format template x...)` | fill `{}` placeholders; `{1}` picks by position, a brace written twice stands for itself, and `{:spec}` takes, in order, a fill character with an alignment (`<`, `>`, `^`), `0` for zero padding, a width, `,` for thousands separators, `.` and a precision, and a type: `e` scientific, `%` percentage, `?` readable form |
| `(string-length s)` | number of characters |
| `(substring s start [end])` | by character position |
| `(string-split s [sep])` | split on a separator, on whitespace when none is given, or into characters when the separator is `""` |
| `(string-join [sep] coll)` | join display forms |
| `(string-upcase s)`, `(string-downcase s)`, `(string-trim s)` | case and trimming |
| `(string-contains? s part)`, `(string-starts-with? s part)`, `(string-ends-with? s part)` | tests |
| `(string-replace s from to)`, `(string-index-of s part)`, `(string-repeat s n)` | editing and search |
| `(string-pad-left s width [ch])`, `(string-pad-right s width [ch])` | pad to a width with spaces or one given character |
| `(string->number s)`, `(number->string n)` | conversions; parsing failure gives `nil` |
| `(name x)`, `(keyword x)`, `(symbol x)` | between strings, keywords, and symbols |
| `(letter? s)`, `(digit? s)`, `(whitespace? s)`, `(upper-case? s)`, `(lower-case? s)` | true when the string is not empty and every character is of that kind |
| `(char-code s)`, `(code-char n)` | the code of a one-character string, and the character for a code |

`count`, `nth`, `first`, and `vec` also work on strings, by character.
Every function above also accepts an array of strings as its first argument
and returns an array, with missing elements staying missing; `format`
returns an array when any argument is one, repeating the scalars.

## Regular expressions

| Function | Description |
|---|---|
| `(re-find s pattern)` | the first match: the matched text, or a vector of the whole match followed by each group when the pattern has groups; `nil` when nothing matches |
| `(re-matches s pattern)` | like `re-find`, but the whole string has to match |
| `(re-seq s pattern)` | every match, as a vector |
| `(re-contains? s pattern)` | whether anything matches |
| `(re-split s pattern)` | the pieces between matches |
| `(re-replace s pattern replacement)` | replace every match with a template, in which `$1` is a group, `$0` the whole match, and `$$` a dollar sign, or with a function that is called on each match |

Patterns are strings, usually raw strings such as `#"\d+"` so the
backslashes stay. They take literals, `.`, classes like `[a-z]` and
`[^0-9]`, `\d`, `\w`, `\s` and their capitals for the opposite, the anchors
`^` and `$`, the word boundaries `\b` and `\B`, groups `(...)` and `(?:...)`,
alternation with `|`, and the repeats `*`, `+`, `?`, `{n}`, `{n,}`, and
`{n,m}`, each made lazy by a following `?`. A pattern starting with `(?i)`
ignores case. There are no backreferences or lookarounds, and matching takes
time proportional to the text, whatever the pattern. The string comes first,
and an array of strings in its place gives an array of results.

## Dates

A date is a calendar day without a time or zone. It reads as `#2024-03-15`,
prints as `2024-03-15`, and sits in arrays and table columns like any other
value. `(+ d n)` and `(- d n)` move by `n` days, `(- d1 d2)` counts the days
between two dates, and `<`, `=`, `min`, `max`, `sort`, and `order-by` compare
dates.

| Function | Description |
|---|---|
| `(date "2024-03-15")`, `(date 2024 3 15)` | from an ISO string or from parts; a day that does not exist is an error |
| `(date s pattern)` | parse with a pattern made of `yyyy`, `yy`, `M`, `MM`, `MMM`, `MMMM`, `d`, `dd`, `EEE`, `EEEE`; every other character must match |
| `(date-format d pattern)` | the same pattern letters, with month and weekday names in English |
| `(year d)`, `(month d)`, `(day d)`, `(weekday d)`, `(day-of-year d)`, `(quarter d)` | parts; `weekday` runs from 1 for Monday to 7 for Sunday |
| `(date-trunc d unit)` | the first day of the `:week` (a Monday), `:month`, `:quarter`, or `:year` |
| `(add-months d n)`, `(add-years d n)` | move by months or years, clamping to the last day of the month |
| `(days-in-month d)`, `(leap-year? d-or-year)` | calendar facts |
| `(date-range start end [unit])` | an array of dates from `start` to `end` inclusive, one per day or per `:week`, `:month`, `:quarter`, or `:year` |
| `(today)` | the current date in UTC |
| `(date? x)` | test |

Every date function also accepts an array of dates, and `date` accepts an
array of strings, so a text column turns into a date column in one call.

## Arrays

| Function | Description |
|---|---|
| `(array coll)` | build an array; the element type is inferred and `nil` marks missing |
| `(array-type a)` | `:int`, `:float`, `:bool`, `:string`, or `:date` |
| `(count-missing a)`, `(drop-missing a)`, `(fill-missing a v)` | missing-value handling |
| `(where coll mask)` | elements where the boolean array is true |
| `(a i)`, `(a mask)`, `(a [i j])` | element, mask selection, or gather by positions |
| `(sum a)`, `(mean a)`, `(median a)`, `(variance a)`, `(std a)` | statistics that skip missing values; `variance` and `std` are sample estimates |
| `(quantile a q)` | linear-interpolation quantile |
| `(running-sum a)`, `(running-max a)`, `(running-min a)` | running aggregates; a missing element stays missing and does not reset the run |
| `(rank a)` | 1-based ascending rank; ties share the lowest rank |
| `(lag a [n])`, `(lead a [n])` | shift by `n` (default 1), filling with `nil` |
| `(diff a)` | change from the previous element |
| `(rolling n f a)` | `f` over each trailing window of `n` elements, `nil` until the window is full |

## Tables

| Function | Description |
|---|---|
| `(table {:col [...] ...})`, `(table :col [...] ...)`, `(table [row-map ...])` | construction |
| `(columns t)`, `(nrows t)`, `(count t)` | shape |
| `(col t :name)`, `(:name t)`, `(t :name)` | a column as an array |
| `(row t i)`, `(rows t)`, `(first t)`, `(map f t)` | rows as maps |
| `(select t [:a :b])`, `(rename t {:old :new})` | columns |
| `(assoc t :c values)`, `(dissoc t :c)`, `(update t :c f)` | add, remove, or transform a column; a scalar value fills the column |
| `(where t mask-or-predicate)` | filter rows |
| `(distinct t [key-or-keys])` | drop duplicate rows, keeping the first; by every column unless keys are given |
| `(order-by t key-or-keys [:asc or :desc])` | stable sort |
| `(head t [n])`, `(tail t [n])` | first or last rows, five by default |
| `(conj t row-map...)`, `(into t rows)`, `(concat t tables-or-rows...)` | append rows; columns are the union, with `nil` where a value is absent |
| `(group-by t key-or-keys)` | map from key to sub-table |
| `(aggregate t [key-or-keys] {:new [f :col] ...})` | summarize, optionally per group; `[f]` applies `f` to the group table |
| `(window t [key-or-keys] {:new [f :col] ...})` | add columns computed per group, kept in the original row order; `f` returns one value per row or a scalar to repeat |
| `(pivot t row-keys :col :value [f])` | long to wide: one column per distinct `:col` value; duplicates are an error unless `f` combines them |
| `(unpivot t [:cols] :key :value)` | wide to long: one row per listed column, with its name in `:key` |
| `(join left right :key [:inner or :left])` | join on a shared column; clashing right columns get a `-right` suffix |
| `(describe t)` | count, missing, mean, std, min, and max per numeric column |

## CSV and output

| Function | Description |
|---|---|
| `(read-csv path [{:sep ";"}])`, `(write-csv t path [opts])` | files; types are inferred per column, ISO dates included, and empty fields are missing |
| `(parse-csv text [opts])`, `(to-csv t [opts])` | the same for strings |
| `(print x...)`, `(println x...)` | human-readable output, strings without quotes |
| `(prn x...)` | output in readable form, strings quoted |
| `(printf template x...)` | print `(format template x...)` as is, with no newline added |
| `(newline)` | a line break |
| `(exit [code])` | stop the program |

## Charts

A chart is built from layers. Each layer verb takes data and returns a
chart, or adds a layer when its first argument is a chart already, so
`(-> (bars s :city :units) (lines s :city :target))` draws lines over bars.
`data` is a table followed by column keywords, or `x` and `y` are two
collections.

| Function | Description |
|---|---|
| `(bars data x y [opts])`, `(bar-chart data x y [opts])` | one bar per category in `x`; `:stack true` stacks the `:by` series, `:normalize true` stacks them to 100%, `:horizontal true` turns the bars sideways, `:sort :desc` orders the categories by value, and `:top n` keeps the largest and sums the rest into Other |
| `(lines data x y [opts])`, `(line-chart data x y [opts])` | points joined in `x` order; `x` may hold numbers or dates; `:step true` holds each value until the next, `:dash true` dashes the line, and `:dots false` leaves the points off |
| `(steps data x y [opts])` | a line that holds each value until the next |
| `(areas data x y [opts])`, `(area-chart data x y [opts])` | a line filled to the baseline; several series stack |
| `(points data x y [opts])`, `(scatter data x y [opts])` | points; `:size :col` scales them by a column, `:shape` picks `:circle`, `:square`, `:diamond`, or `:triangle`, `:shapes true` gives each series its own, and `:text :col` labels them |
| `(lollipops data x y [opts])` | bars drawn as a stem with a dot |
| `(pie data x y [opts])`, `(pie-chart data x y [opts])` | one slice per category, duplicates summed; `:donut true` leaves a hole |
| `(histogram data [:col] [opts])` | counts of numbers in equal-width bins, ten unless `:bins` says otherwise |
| `(density data [:col] [opts])` | a smoothed distribution; `:bandwidth` widens or narrows the kernel |
| `(box-plot data [:group] :value [opts])`, `(boxes data [:group] :value [opts])`, `(box-plot numbers [opts])` | median, quartiles, whiskers at 1.5 times the interquartile range, and outliers as points |
| `(heatmap table x y value [opts])` | one shaded cell per `x` and `y` category |
| `(plot [opts])` | an empty chart to add layers to |
| `(title plot text [subtitle])`, `(subtitle plot text)`, `(caption plot text)` | a heading, a line under it, and a note at the foot |
| `(x-axis plot opts)`, `(y-axis plot opts)`, `(right-axis plot opts)` | axis settings as a map of `:label`, `:range [low high]`, `:scale :log`, `:format`, and `:ticks n`; a layer given `:axis :right` uses the right axis |
| `(hline plot value [label])`, `(vline plot value [label])` | dashed reference lines |
| `(annotate plot x y text)` | a note at a point |
| `(legend plot where)` | `:top`, `:right`, `:bottom`, `:none`, or `:auto` |
| `(palette plot colors)` | a vector of `"#rrggbb"` strings, or `:default`, `:warm`, `:cool`, `:earth`, `:gray`, or `:bright` |
| `(trend plot [opts])` | a dashed least-squares line through the last layer |
| `(smooth plot n)` | an `n`-point rolling mean of the last layer |
| `(figure plots [opts])` | several charts in one picture, with `:columns` and `:title` |
| `(plot? x)` | test |
| `(to-svg plot [opts])`, `(save-plot plot path [opts])` | the chart as an SVG document, as a string or written to a file; `:width` and `:height` default to 640 by 400, and `:theme :dark` draws on a dark background |
| `(plot-theme! opts)`, `(plot-theme)` | set or read the defaults every new chart starts from: `:palette`, `:grid`, and `:legend` |

Options are a map, and what a decorator sets can also be given to a layer
verb. `:title`, `:subtitle`, `:caption`, `:x-label`, and `:y-label` name
things; `:by` splits the rows into series by a column and `:facet` splits
them into panels with shared axes. `:color "#rrggbb"`, `:width`, and
`:opacity` style a layer, `:labels true` prints each value next to its bar
or point, `:low` and `:high` name columns that draw error bars, and `:trend
true` or `:rolling n` add a fit or a rolling mean in one step. Axes take
`:x-range` and `:y-range` as `[low high]`, `:x-scale` and `:y-scale` as
`:linear` or `:log`, `:x-ticks` and `:y-ticks` as a count, and `:x-format`
and `:y-format` as a `format` spec such as `"{:,}"` or `"{:.0%}"`, as
`:compact` for `1.5K` and `2M`, or as a `date-format` pattern for a date
axis. `:hline` and `:vline` draw reference lines at one value or a
collection of them, and `:legend`, `:palette`, and `:grid false` do what the
decorators do. A chart is a value: printing it draws a text version, which
at a terminal uses fine blocks, braille lines, and the series colors;
Schematic Studio shows it in the Plots pane, where hovering reads every
series at a point, clicking a legend entry hides its series, clicking a bar
filters the data grid to that category, and dragging a box zooms in; and
missing values are left out.

## Namespaces

Every name lives in a namespace. Builtins live in `schematic.core`, which
every namespace sees, and code that does not say otherwise runs in `user`.
A symbol written as `prefix/name` looks up `name` in the namespace that
`prefix` is an alias for, or in the namespace called `prefix`, unless the
whole symbol is itself a name, which is how `call/ec` and `/` keep working.

| Function | Description |
|---|---|
| `(ns name clauses...)` | switch to the namespace `name`, creating it if needed; a `(:require spec...)` clause loads other namespaces as `require` does |
| `(require spec...)` | load namespaces; a spec is `'my.lib`, `'[my.lib :as m]`, `'[my.lib :refer [f g]]`, or `'[my.lib :refer :all]`, and `my.lib` is read from `my/lib.scm` on the load path |
| `(refer 'ns)`, `(refer 'ns :only '[f g])` | make a loaded namespace's names, or just some of them, usable without a prefix |
| `(alias 'short 'my.lib)` | let `short/name` stand for `my.lib/name` |
| `(load path)` | run a file in the current namespace; a relative path is looked up on the load path |
| `(current-ns)` | the current namespace as a symbol |

The load path starts with the folder of the file being run and the working
directory. A file loaded for a namespace begins with its own `ns` form, and
loading it leaves the current namespace unchanged. Defining a name that
already exists in `schematic.core` shadows it from then on in that
namespace; code compiled earlier keeps the builtin.

## Projects

A project is a folder with a `project.scm` manifest, a `src` folder of
namespaces, and, once it has dependencies, a `packages` folder holding their
sources and a `project.lock` pinning them. Running a file inside a project,
or `schematic run`, puts `src` and every package's source folder on the load
path, so `(require 'text-tools)` finds a dependency called `text-tools`.

| Command | Effect |
|---|---|
| `schematic new <name>` | create `<name>/project.scm` and `<name>/src/<name>.scm` |
| `schematic add <name>[@version]` | add a package from the registry, the newest version unless one is given: its source is copied into `packages/<name>` and the commit and a hash of the files are pinned in `project.lock` |
| `schematic add <name> --path <dir>` | depend on a local folder, used in place |
| `schematic add <name> --git <url> --tag <t>`, `schematic add <name> --git <url> --commit <c> --subdir <d>` | depend on a git repository, or on one folder of it |
| `schematic remove <name>` | drop a dependency, and every vendored package nothing else needs |
| `schematic restore` | fetch whatever `packages` is missing or has changed, at the pinned commits, and pin anything new, including what the packages themselves depend on |
| `schematic restore --locked` | the same, but refuse to resolve anything `project.lock` does not pin, and fail when a fetched package does not hash as the lock says |
| `schematic packages` | list every package in use, direct or brought in by another package, with its version, license, and status |
| `schematic run [file]` | run the project's main namespace, `src/<name>.scm` unless `:main` names another, or a given file |
| `schematic check [file...]` | report certain mistakes without running anything; with no file, every script under the project's source folder |

`project.scm` is a map with `:name`, `:version`, an optional `:license`
(an SPDX expression such as `"MIT"` or `"MIT OR Apache-2.0"`), `:source`
(`"src"` by default), `:main`, and `:registry`, and a `:dependencies` map
from names to a version string, `{:path "../lib"}`, or `{:git url :tag t
:subdir d}`. A package is a project too, so its own `:dependencies` come
along when it is added; the project's choice of a version wins over a
package's, and among packages the newest requested version wins. With
`:allowed-licenses ["MIT" "Apache-2.0"]` in the manifest, `add` and
`restore` refuse any package whose license is missing or outside the list.
`project.lock` records, per package, the version, the license, the git
source and commit, the names it depends on, and a `sha256:` hash of its
files, which ignores line endings; packages are vendored, so a checked-in
project builds offline. The registry is the repository
`collide2954/schematic-registry`, read over HTTPS one file per package;
`:registry` in the manifest or the `SCHEMATIC_REGISTRY` variable can name
another one, including a local folder.

## Types

`schematic check` and Studio's editor read code without running it and
report only what is certain to fail: a call with the wrong number of
arguments, an argument whose type the function can never accept, a name
that is never defined, a value that is not a function being called, a form
the compiler rejects, and an annotation that does not hold. Types are
inferred from literals, definitions, and the built-in functions; whatever
cannot be inferred stays dynamic and is never reported, and so is code
inside a `try` that has a `catch`. Annotations are optional, and running
code ignores them.

| Form | Meaning |
|---|---|
| `[name : type]` | annotate a parameter, a binding in `let`, `let*`, `loop`, a named `let`, or `match`, or a defined name; a pattern can stand in for `name` |
| `(define (f [x : int]) : string body...)`, `(lambda ([x : int]) : string body...)` | declare what a function returns, checked against its body and at every call |
| `(define name : type value)` | declare a value's type |

A type is written `int`, `float`, `number` (either), `string`, `bool`,
`keyword`, `symbol`, `date`, `nil`, `table`, `plot`, `fn`, or `any`, which
turns checking off for that name. `[int]` is a vector of integers, `{keyword
int}` a map from keywords to integers, `#{string}` a set, `#[float]` an
array, `(list int)` a list, `(or int nil)` one of several types, and `(->
int int string)` a function from two integers to a string.

## Help

| Function | Description |
|---|---|
| `(doc name)` | print the reference entry for `name`, which is not evaluated |
| `(doc-string name)` | the same text as a string, or `nil` when there is none; `name` may be a symbol, keyword, or string |
| `(apropos part)` | a vector of the documented names that contain `part` |

## Errors

| Function | Description |
|---|---|
| `(error message [data])` | raise a map with `:kind :user`, `:message`, and optional `:data` |
| `(raise value)` | raise any value |

Runtime errors are maps whose `:kind` is one of `:type-error`,
`:arity-error`, `:arithmetic`, `:index-error`, `:value-error`, `:unbound`,
`:syntax-error`, `:read-error`, `:io-error`, or `:stack-overflow`.
