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
fn integer_arithmetic() {
    assert_eq!(eval("(+ 1 2)"), "3");
    assert_eq!(eval("(+)"), "0");
    assert_eq!(eval("(+ 1 2 3 4)"), "10");
    assert_eq!(eval("(- 10 3)"), "7");
    assert_eq!(eval("(- 5)"), "-5");
    assert_eq!(eval("(- 10 1 2)"), "7");
    assert_eq!(eval("(* 2 3 4)"), "24");
    assert_eq!(eval("(*)"), "1");
}

#[test]
fn floats_contaminate_arithmetic() {
    assert_eq!(eval("(+ 1 2.5)"), "3.5");
    assert_eq!(eval("(* 2 0.5)"), "1.0");
    assert_eq!(eval("(- 1.5)"), "-1.5");
    assert_eq!(eval("(- 3 0.5)"), "2.5");
}

#[test]
fn division_always_yields_a_float() {
    assert_eq!(eval("(/ 6 3)"), "2.0");
    assert_eq!(eval("(/ 1 2)"), "0.5");
    assert_eq!(eval("(/ 2)"), "0.5");
    assert_eq!(eval("(/ 12 2 3)"), "2.0");
    assert_eq!(eval("(/ 1 0)"), "+inf.0");
    assert_eq!(eval("(/ 0 0)"), "+nan.0");
}

#[test]
fn integer_division_and_remainders() {
    assert_eq!(eval("(quot 7 2)"), "3");
    assert_eq!(eval("(quot -7 2)"), "-3");
    assert_eq!(eval("(rem -7 2)"), "-1");
    assert_eq!(eval("(mod -7 2)"), "1");
    assert_eq!(eval("(mod 7 -2)"), "-1");
    assert_eq!(eval("(quot 7.5 2)"), "3.0");
    assert_eq!(eval("(quot 1 0)"), "error: division by zero");
    assert_eq!(eval("(mod 7 0)"), "error: division by zero");
}

#[test]
fn integer_overflow_is_an_error() {
    assert_eq!(eval("(+ 9223372036854775807 1)"), "error: integer overflow");
    assert_eq!(eval("(* 4611686018427387904 2)"), "error: integer overflow");
    assert_eq!(eval("(- -9223372036854775808)"), "error: integer overflow");
    assert_eq!(eval("(quot -9223372036854775808 -1)"), "error: integer overflow");
}

#[test]
fn comparisons_chain_across_numbers() {
    assert_eq!(eval("(< 1 2 3)"), "true");
    assert_eq!(eval("(< 1 3 2)"), "false");
    assert_eq!(eval("(<= 1 1 2)"), "true");
    assert_eq!(eval("(> 3 2 1)"), "true");
    assert_eq!(eval("(>= 2 2 1)"), "true");
    assert_eq!(eval("(< 1 2.5)"), "true");
    assert_eq!(eval("(< 1)"), "true");
    assert_eq!(eval("(< 1 'a)"), "error: < expects numbers, got a");
}

#[test]
fn equality_is_structural_and_numeric() {
    assert_eq!(eval("(= 1 1)"), "true");
    assert_eq!(eval("(= 1 1.0)"), "true");
    assert_eq!(eval("(= 1 2)"), "false");
    assert_eq!(eval("(= [1 2] [1 2])"), "true");
    assert_eq!(eval("(= '(1) [1])"), "false");
    assert_eq!(eval("(= {:a 1} {:a 1})"), "true");
    assert_eq!(eval("(= 1 1 1)"), "true");
    assert_eq!(eval("(= 1 1 2)"), "false");
    assert_eq!(eval("(not true)"), "false");
    assert_eq!(eval("(not nil)"), "true");
    assert_eq!(eval("(not 0)"), "false");
}

#[test]
fn arithmetic_type_errors_name_the_offender() {
    assert_eq!(eval("(+ 1 \"a\")"), "error: + expects numbers, got \"a\"");
    assert_eq!(eval("(* :k 2)"), "error: * expects numbers, got :k");
}

#[test]
fn builtins_are_values() {
    assert_eq!(eval("+"), "#<builtin +>");
    assert_eq!(eval("(define plus +) (plus 1 2)"), "3");
}

#[test]
fn calling_a_non_function_is_an_error() {
    assert_eq!(eval("(1 2)"), "error: 1 is not a function");
    assert_eq!(eval("(+ 1 2 3 4 5)"), "15");
    assert_eq!(eval("(not)"), "error: not expects 1 argument, got 0");
}

#[test]
fn quote_wants_exactly_one_form() {
    assert!(eval("(quote)").starts_with("error:"));
    assert!(eval("(quote 1 2)").starts_with("error:"));
}
