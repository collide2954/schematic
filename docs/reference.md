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
| `(format template x...)` | fill `{}` placeholders; `{1}` picks by position, `{{` is a literal brace, and `{:spec}` takes, in order, a fill character with an alignment (`<`, `>`, `^`), `0` for zero padding, a width, `,` for thousands separators, `.` and a precision, and a type: `e` scientific, `%` percentage, `?` readable form |
| `(string-length s)` | number of characters |
| `(substring s start [end])` | by character position |
| `(string-split s [sep])` | split on a separator, or on whitespace |
| `(string-join [sep] coll)` | join display forms |
| `(string-upcase s)`, `(string-downcase s)`, `(string-trim s)` | case and trimming |
| `(string-contains? s part)`, `(string-starts-with? s part)`, `(string-ends-with? s part)` | tests |
| `(string-replace s from to)`, `(string-index-of s part)`, `(string-repeat s n)` | editing and search |
| `(string-pad-left s width [ch])`, `(string-pad-right s width [ch])` | pad to a width with spaces or one given character |
| `(string->number s)`, `(number->string n)` | conversions; parsing failure gives `nil` |
| `(name x)`, `(keyword x)`, `(symbol x)` | between strings, keywords, and symbols |

`count`, `nth`, `first`, and `vec` also work on strings, by character.
Every function above also accepts an array of strings as its first argument
and returns an array, with missing elements staying missing; `format`
returns an array when any argument is one, repeating the scalars.

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

| Function | Description |
|---|---|
| `(bar-chart data x y [opts])` | one bar per category in `x`; `data` is a table with two column keywords, or `x` and `y` are two collections |
| `(line-chart data x y [opts])` | points joined in `x` order; `x` may hold numbers or dates |
| `(scatter data x y [opts])` | points |
| `(histogram data [:col] [opts])` | counts of numbers in equal-width bins, ten unless `:bins` says otherwise |
| `(plot? x)` | test |
| `(to-svg plot)`, `(save-plot plot path)` | the chart as a 640 by 400 SVG document, as a string or written to a file |

Options are a map with `:title`, `:x-label`, `:y-label`, `:by` (a column
whose values split the rows into series), and `:bins`. A chart is a value:
printing it draws a text version, Schematic Studio shows it in the Plots
pane, and missing values are left out.

## Errors

| Function | Description |
|---|---|
| `(error message [data])` | raise a map with `:kind :user`, `:message`, and optional `:data` |
| `(raise value)` | raise any value |

Runtime errors are maps whose `:kind` is one of `:type-error`,
`:arity-error`, `:arithmetic`, `:index-error`, `:value-error`, `:unbound`,
`:syntax-error`, `:read-error`, `:io-error`, or `:stack-overflow`.
