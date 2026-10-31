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
fn lambdas_are_called_with_arguments() {
    assert_eq!(eval("((lambda (x) x) 5)"), "5");
    assert_eq!(eval("((lambda (x y) (- x y)) 10 3)"), "7");
    assert_eq!(eval("((lambda () 1))"), "1");
    assert_eq!(eval("((lambda (x) (* x 2) (+ x 1)) 5)"), "6");
}

#[test]
fn define_has_a_function_shorthand() {
    assert_eq!(eval("(define (square x) (* x x)) (square 4)"), "16");
    assert_eq!(eval("(define (square x) (* x x)) square"), "#<procedure square>");
    assert_eq!(eval("(define cube (lambda (x) (* x x x))) cube"), "#<procedure cube>");
    assert_eq!(eval("(lambda (x) x)"), "#<procedure>");
}

#[test]
fn closures_capture_their_environment() {
    assert_eq!(eval("(define (adder n) (lambda (x) (+ x n))) ((adder 5) 10)"), "15");
    assert_eq!(eval("(define add5 ((lambda (n) (lambda (x) (+ x n))) 5)) [(add5 1) (add5 2)]"), "[6 7]");
    assert_eq!(eval("(define (f a) (lambda (b) (lambda (c) (+ a b c)))) (((f 1) 2) 3)"), "6");
}

#[test]
fn recursion_works_through_globals() {
    assert_eq!(eval("(define (fact n) (if (< n 2) 1 (* n (fact (- n 1))))) (fact 10)"), "3628800");
    assert_eq!(eval("(define (fib n) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2))))) (fib 15)"), "610");
}

#[test]
fn arity_errors_name_the_function() {
    assert_eq!(eval("(define (f x) x) (f)"), "error: f expects 1 argument, got 0");
    assert_eq!(eval("((lambda (x y) x) 1)"), "error: procedure expects 2 arguments, got 1");
}

#[test]
fn variadic_parameters_collect_a_list() {
    assert_eq!(eval("(define (f a & rest) rest) (f 1 2 3)"), "(2 3)");
    assert_eq!(eval("(define (f a & rest) rest) (f 1)"), "()");
    assert_eq!(eval("((lambda args args) 1 2)"), "(1 2)");
    assert_eq!(eval("(define (f a & rest) a) (f)"), "error: f expects at least 1 argument, got 0");
}

#[test]
fn functions_are_first_class() {
    assert_eq!(eval("(define (twice f x) (f (f x))) (twice (lambda (x) (* x 2)) 3)"), "12");
    assert_eq!(
        eval("(define (compose f g) (lambda (x) (f (g x)))) ((compose (lambda (x) (+ x 1)) (lambda (x) (* x 2))) 5)"),
        "11"
    );
    assert_eq!(eval("(define (apply-to-five f) (f 5)) (apply-to-five -)"), "-5");
}

#[test]
fn parameters_shadow_globals_and_special_forms() {
    assert_eq!(eval("(define x 1) ((lambda (x) x) 2)"), "2");
    assert_eq!(eval("((lambda (if) (if 1 2)) -)"), "-1");
}

#[test]
fn lambda_syntax_errors() {
    assert!(eval("(lambda)").starts_with("error:"));
    assert!(eval("(lambda (1) 1)").starts_with("error:"));
    assert!(eval("(lambda (x))").starts_with("error:"));
    assert!(eval("(lambda (x x) x)").starts_with("error:"));
    assert!(eval("(lambda (x &) x)").starts_with("error:"));
}

#[test]
fn quote_wants_exactly_one_form() {
    assert!(eval("(quote)").starts_with("error:"));
    assert!(eval("(quote 1 2)").starts_with("error:"));
}
