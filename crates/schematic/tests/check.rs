use std::path::{Path, PathBuf};

use schematic::check::check_source;

fn problems(src: &str) -> Vec<String> {
    check_source(src, None, &[]).iter().map(ToString::to_string).collect()
}

#[test]
fn clean_code_has_no_problems() {
    let src = "(define rate 0.25)
(define (with-tax [price : number]) : number (* price (+ 1 rate)))
(define names [\"Ada\" \"Bob\"])
(define (greet n) (str \"Hi \" (string-upcase n)))
(let ((total (reduce + 0 (map with-tax [10 20]))))
  (println (format \"{:.1}\" total))
  (when (> total 30) (greet (first names))))
(define t (table {:city [\"Oslo\"] :units [3]}))
(:units (where t (> (:units t) 1)))
(cond ((nil? rate) \"none\") (else (max rate 1)))
(match [1 2] ([a b] (+ a b)) (else 0))
(try (/ 1 0) (catch e (:message e)))
(loop ((i 0) (acc [])) (if (= i 3) acc (recur (+ i 1) (conj acc i))))";
    assert_eq!(problems(src), Vec::<String>::new());
}

#[test]
fn the_tour_checks_clean() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join("tour.scm");
    let src = std::fs::read_to_string(&path).unwrap();
    assert_eq!(check_source(&src, path.parent(), &[]).len(), 0);
}

#[test]
fn wrong_argument_types_are_reported() {
    assert_eq!(problems("(+ 1 \"two\")"), ["1:6: + expects a number, got a string"]);
    assert_eq!(problems("(string-length 42)"), ["1:16: string-length expects a string, got an integer"]);
    assert_eq!(problems("(define n 3)\n(string-upcase n)"), ["2:16: string-upcase expects a string, got an integer"]);
    assert_eq!(problems("(string-upcase #[\"a\" nil]) (+ #2024-01-01 1) (count nil) (first nil)"), Vec::<String>::new());
    assert_eq!(problems("(+ 1 nil)"), ["1:6: + expects a number, got nil"]);
}

#[test]
fn arity_mistakes_are_reported() {
    assert_eq!(problems("(define (sq x) (* x x))\n(sq 1 2)"), ["2:1: sq expects 1 argument, got 2"]);
    assert_eq!(problems("(string-length)"), ["1:1: string-length expects 1 argument, got 0"]);
    assert_eq!(problems("(define (f & xs) xs) (f) (f 1 2) (define (g a & b) a) (g 1)"), Vec::<String>::new());
    assert_eq!(problems("(define (g a & b) a) (g)"), ["1:22: g expects at least 1 argument, got 0"]);
}

#[test]
fn calling_a_non_function_is_reported() {
    assert_eq!(problems("(define x 5)\n(x 1)"), ["2:1: x is an integer, not a function"]);
    assert_eq!(problems("(\"s\" 1)"), ["1:1: \"s\" is a string, not a function"]);
    assert_eq!(problems("({:a 1} :a) ([1 2] 0) (:a {}) (#{1} 1) ((table {:a [1]}) :a)"), Vec::<String>::new());
}

#[test]
fn unknown_names_are_reported_unless_code_is_loaded_dynamically() {
    assert_eq!(problems("(define (f) (g 1))"), ["1:14: g is not defined"]);
    assert_eq!(problems("(define (f) (g 1)) (define (g x) x)"), Vec::<String>::new());
    assert_eq!(problems("(load \"other.scm\") (whatever 1)"), Vec::<String>::new());
    assert_eq!(problems("(ns a (:require [b :refer :all])) (whatever 1)"), Vec::<String>::new());
    assert_eq!(problems("(ns a (:require [b :as bb])) (bb/thing 1)"), Vec::<String>::new());
}

#[test]
fn annotations_are_checked_at_calls_and_returns() {
    assert_eq!(problems("(define (area [r : float]) : float (* r r))\n(area \"big\")"), ["2:7: area expects a float for r, got a string"]);
    assert_eq!(
        problems("(define (shout [s : string]) : string (string-length s))"),
        ["1:39: shout is declared to return a string but returns an integer"]
    );
    assert_eq!(problems("(define (f [x : (or int nil)]) (if x (+ x 1) 0)) (f nil) (f 2)"), Vec::<String>::new());
    assert_eq!(problems("(define (f [v : [int]]) (first v)) (f [1 2]) (f \"ab\")"), ["1:49: f expects a vector of integers for v, got a string"]);
    assert_eq!(problems("(define total : int \"x\")"), ["1:21: total is declared an integer but given a string"]);
    assert_eq!(problems("(define (f [x : whatever]) x)"), ["1:17: unknown type whatever"]);
}

#[test]
fn inference_flows_through_let_if_and_defines() {
    assert_eq!(
        problems("(define name \"Ada\")\n(let ((n (string-length name))) (string-upcase n))"),
        ["2:48: string-upcase expects a string, got an integer"]
    );
    assert_eq!(problems("(define (pick flag) (if flag 1 \"one\"))\n(string-upcase (pick true))"), Vec::<String>::new());
    assert_eq!(problems("(define x (if true 1 2))\n(string-upcase x)"), ["2:16: string-upcase expects a string, got an integer"]);
    assert_eq!(problems("(define x 1) (set! x \"s\") (string-upcase x)"), Vec::<String>::new());
    assert_eq!(problems("(define (f) (define inner 2) (string-upcase inner))"), ["1:45: string-upcase expects a string, got an integer"]);
}

#[test]
fn syntax_and_read_errors_get_positions() {
    let found = problems("(define 1 2)\n(if)");
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found[0].starts_with("1:1: define") && found[1].starts_with("2:1: if"), "{found:?}");
    assert_eq!(problems("(+ 1"), ["1:2: unclosed `(`"]);
}

#[test]
fn namespaces_resolve_across_required_files() {
    let dir = std::env::temp_dir().join(format!("schematic-check-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("lib.scm"), "(ns lib)\n(define (twice [n : int]) : int (* 2 n))\n").unwrap();
    let src = "(ns app (:require [lib :as l :refer [twice]]))\n(l/twice \"x\")\n(twice 1 2)\n(l/nothing)\n(twice 4)";
    let found: Vec<String> = check_source(src, Some(&dir), &[PathBuf::from(&dir)]).iter().map(ToString::to_string).collect();
    assert_eq!(
        found,
        ["2:10: l/twice expects an integer for n, got a string", "3:1: twice expects 1 argument, got 2", "4:2: l/nothing is not defined"]
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
