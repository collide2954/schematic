use schematic::Interpreter;

fn eval(src: &str) -> String {
    match Interpreter::new().eval_str(src) {
        Ok(v) => v.to_string(),
        Err(e) => format!("error: {e}"),
    }
}

#[test]
fn literals_evaluate_to_themselves() {
    assert_eq!(eval("42"), "42");
    assert_eq!(eval("2.5"), "2.5");
    assert_eq!(eval("\"hi\""), "\"hi\"");
    assert_eq!(eval("true"), "true");
    assert_eq!(eval("false"), "false");
    assert_eq!(eval("nil"), "nil");
    assert_eq!(eval(":k"), ":k");
}

#[test]
fn quote_returns_data_unevaluated() {
    assert_eq!(eval("'x"), "x");
    assert_eq!(eval("'(1 x \"s\")"), "(1 x \"s\")");
    assert_eq!(eval("(quote [a b])"), "[a b]");
    assert_eq!(eval("()"), "()");
}

#[test]
fn collection_literals_evaluate_their_elements() {
    assert_eq!(eval("[1 'x [2]]"), "[1 x [2]]");
    assert_eq!(eval("{:a 'x, :b [1]}"), "{:a x, :b [1]}");
    assert_eq!(eval("#{'a}"), "#{a}");
}

#[test]
fn the_last_form_is_the_result() {
    assert_eq!(eval("1 2 3"), "3");
    assert_eq!(eval(""), "nil");
}

#[test]
fn read_errors_are_reported() {
    assert!(eval("(1 2").starts_with("error:"));
}

#[test]
fn unbound_symbols_are_errors() {
    assert_eq!(eval("nope"), "error: nope is not defined");
}

#[test]
fn if_picks_a_branch_by_truthiness() {
    assert_eq!(eval("(if true 1 2)"), "1");
    assert_eq!(eval("(if false 1 2)"), "2");
    assert_eq!(eval("(if nil 1 2)"), "2");
    assert_eq!(eval("(if 0 1 2)"), "1");
    assert_eq!(eval("(if '() 1 2)"), "1");
    assert_eq!(eval("(if \"\" 1 2)"), "1");
}

#[test]
fn if_without_else_gives_nil() {
    assert_eq!(eval("(if false 1)"), "nil");
    assert_eq!(eval("(if true 1)"), "1");
}

#[test]
fn if_nests_in_any_position() {
    assert_eq!(eval("(if (if true false true) 'a (if nil 'b 'c))"), "c");
    assert_eq!(eval("[(if true 1 2) (if false 1 2)]"), "[1 2]");
}

#[test]
fn if_arity_errors() {
    assert!(eval("(if true)").starts_with("error:"));
    assert!(eval("(if 1 2 3 4)").starts_with("error:"));
}

#[test]
fn define_creates_globals() {
    assert_eq!(eval("(define x 10) x"), "10");
    assert_eq!(eval("(define x 10)"), "x");
    assert_eq!(eval("(define x 1) (define y x) y"), "1");
    assert_eq!(eval("(define x 1) (define x 2) x"), "2");
}

#[test]
fn set_changes_an_existing_global() {
    assert_eq!(eval("(define x 1) (set! x 2) x"), "2");
    assert_eq!(eval("(define x 1) (set! x 5)"), "5");
    assert_eq!(eval("(set! nope 1)"), "error: nope is not defined");
}

#[test]
fn begin_runs_forms_in_order_and_returns_the_last() {
    assert_eq!(eval("(begin 1 2 3)"), "3");
    assert_eq!(eval("(begin)"), "nil");
    assert_eq!(eval("(begin (define x 1) (set! x (if x 2 3)) x)"), "2");
    assert_eq!(eval("[(begin 1 2) 3]"), "[2 3]");
}

#[test]
fn define_and_set_syntax_errors() {
    assert!(eval("(define)").starts_with("error:"));
    assert!(eval("(define 1 2)").starts_with("error:"));
    assert!(eval("(set! x)").starts_with("error:"));
}

#[test]
fn quote_wants_exactly_one_form() {
    assert!(eval("(quote)").starts_with("error:"));
    assert!(eval("(quote 1 2)").starts_with("error:"));
}
