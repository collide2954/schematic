use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

use schematic::Interpreter;

#[derive(Clone, Default)]
struct Captured(Rc<RefCell<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn eval_with_output(src: &str) -> (String, String) {
    let captured = Captured::default();
    let mut interp = Interpreter::new();
    interp.set_output(Box::new(captured.clone()));
    let result = match interp.eval_str(src) {
        Ok(v) => v.to_string(),
        Err(e) => format!("error: {e}"),
    };
    let output = String::from_utf8(captured.0.borrow().clone()).unwrap();
    (result, output)
}

fn eval(src: &str) -> String {
    match Interpreter::new().eval_str(src) {
        Ok(v) => v.to_string(),
        Err(e) => format!("error: {e}"),
    }
}

#[test]
fn user_globals_are_listed_in_definition_order() {
    let mut interp = Interpreter::new();
    assert!(interp.user_globals().next().is_none());
    interp.eval_str("(define a 1) (define (f x) x) (define a 2)").unwrap();
    let names: Vec<String> = interp.user_globals().map(|(name, _)| name).collect();
    assert_eq!(names, ["a", "f"]);
    assert_eq!(interp.global("a").unwrap().to_string(), "2");
    assert!(interp.global("map").is_some());
    assert!(interp.global("zzz").is_none());
}

#[test]
fn a_raised_interrupt_flag_stops_running_code() {
    use std::sync::atomic::Ordering;
    let mut interp = Interpreter::new();
    let flag = interp.interrupt_handle();
    let interrupted = |interp: &mut Interpreter, src: &str| {
        flag.store(true, Ordering::Relaxed);
        let result = interp.eval_str(src).map(|v| v.to_string());
        (result.unwrap_err().to_string(), flag.load(Ordering::Relaxed))
    };
    assert_eq!(interrupted(&mut interp, "(let loop ((i 0)) (loop (+ i 1)))"), ("interrupted".to_string(), false));
    assert_eq!(interp.eval_str("(+ 1 2)").unwrap().to_string(), "3");
    assert_eq!(interrupted(&mut interp, "(loop ((i 0)) (recur (+ i 1)))").0, "interrupted");
    assert_eq!(interrupted(&mut interp, "(define (g) (+ 1 (g))) (g)").0, "interrupted");
    assert_eq!(interrupted(&mut interp, "(try (let loop () (loop)) (catch e :caught))").0, "interrupted");
    assert_eq!(interrupted(&mut interp, "(try (map (lambda (x) (let loop () (loop))) [1]) (catch e :caught))").0, "interrupted");
    let captured = Captured::default();
    interp.set_output(Box::new(captured.clone()));
    assert_eq!(interrupted(&mut interp, "(try (let loop () (loop)) (finally (println \"cleanup\")))").0, "interrupted");
    assert_eq!(String::from_utf8(captured.0.borrow().clone()).unwrap(), "cleanup\n");
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
fn arithmetic_respects_shadowing_and_redefinition() {
    assert_eq!(eval("(let ((+ -)) (+ 5 3))"), "2");
    assert_eq!(eval("(define (f + a b) (+ a b)) (f * 3 4)"), "12");
    assert_eq!(eval("(define (f) (define < >) (< 1 2)) (f)"), "false");
    assert_eq!(eval("(define + -) (+ 5 3)"), "2");
    assert_eq!(eval("(define (plus a b) (+ a b)) (define + -) (plus 5 3)"), "8");
    assert_eq!(eval("(map + [1 2] [3 4])"), "[4 6]");
    assert_eq!(eval("[(not 1) (= 1 1 1) (< 1 2 3) (+ 1 2 3)]"), "[false true true 6]");
}

#[test]
fn fused_arithmetic_keeps_jump_targets_and_semantics() {
    assert_eq!(eval("(define (f c x y) (+ (if c x y) 1)) [(f true 1 10) (f false 1 10)]"), "[2 11]");
    assert_eq!(eval("(define (f c) (if (< (if c 0 5) 1) 'small 'big)) [(f true) (f false)]"), "[small big]");
    assert_eq!(eval("(define (g a b) (- a b)) (define (h a) (* 3 a)) (define (k a) (- a 1.5)) [(g 10 3) (h 4) (k 1)]"), "[7 12 -0.5]");
    assert_eq!(eval("(let ((x 7)) [(+ x 1) (< x 10) (= x 7) (* x x)])"), "[8 true true 49]");
    assert_eq!(eval("(define (p a) (< a \"s\")) (p 1)"), "error: < expects numbers, got \"s\"");
    assert_eq!(eval("(define (q a) (+ a 1)) (q 9223372036854775807)"), "error: integer overflow");
    assert_eq!(eval("(define (r a b) (if (= a b) (+ a b) (- a b))) [(r 2 2) (r 5 2)]"), "[4 3]");
    assert_eq!(eval("(define (s v) (+ v 1)) (s #[1 2])"), "#[2 3]");
    assert_eq!(eval("(loop ((i 0) (acc 0)) (if (< i 10) (recur (+ i 1) (+ acc i)) acc))"), "45");
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

fn eval_with_frame_limit(src: &str, limit: usize) -> String {
    let mut interp = Interpreter::new();
    interp.set_frame_limit(limit);
    match interp.eval_str(src) {
        Ok(v) => v.to_string(),
        Err(e) => format!("error: {e}"),
    }
}

#[test]
fn tail_calls_do_not_grow_the_stack() {
    let count = "(define (count n) (if (= n 0) 'done (count (- n 1)))) (count 10000)";
    assert_eq!(eval_with_frame_limit(count, 100), "done");
    let mutual = "(define (even? n) (if (= n 0) true (odd? (- n 1)))) (define (odd? n) (if (= n 0) false (even? (- n 1)))) (even? 10001)";
    assert_eq!(eval_with_frame_limit(mutual, 100), "false");
    let through_begin = "(define (count n) (begin 1 (if (= n 0) 'done (count (- n 1))))) (count 10000)";
    assert_eq!(eval_with_frame_limit(through_begin, 100), "done");
}

#[test]
fn deep_non_tail_recursion_works_until_the_limit() {
    let sum = "(define (sum n) (if (= n 0) 0 (+ n (sum (- n 1))))) (sum 500)";
    assert_eq!(eval_with_frame_limit(sum, 1000), "125250");
    assert_eq!(eval("(define (sum n) (if (= n 0) 0 (+ n (sum (- n 1))))) (sum 20000)"), "200010000");
    let runaway = "(define (f) (+ 1 (f))) (f)";
    assert_eq!(eval_with_frame_limit(runaway, 1000), "error: stack overflow");
}

#[test]
fn let_binds_locals_in_parallel() {
    assert_eq!(eval("(let ((x 1) (y 2)) (+ x y))"), "3");
    assert_eq!(eval("(define x 10) (let ((x 1) (y x)) y)"), "10");
    assert_eq!(eval("(let () 5)"), "5");
    assert_eq!(eval("(let ((x 1)) (let ((x 2)) x))"), "2");
    assert_eq!(eval("(let ((x 1)) (let ((y 2)) x))"), "1");
    assert_eq!(eval("(let ((x 1)) 'ignored x)"), "1");
}

#[test]
fn let_works_with_temporaries_beneath_it() {
    assert_eq!(eval("[(let ((x 1)) x) 2]"), "[1 2]");
    assert_eq!(eval("(+ 100 (let ((x 1) (y 2)) (* x y)))"), "102");
    assert_eq!(eval("(- (let ((a 5)) a) (let ((b 2)) b))"), "3");
}

#[test]
fn let_star_binds_sequentially() {
    assert_eq!(eval("(let* ((x 1) (y (+ x 1))) y)"), "2");
    assert_eq!(eval("(let* ((x 1) (x (+ x 1))) x)"), "2");
    assert_eq!(eval("(let* () 7)"), "7");
}

#[test]
fn let_locals_can_be_captured() {
    assert_eq!(eval("(define f (let ((n 5)) (lambda (x) (+ x n)))) (f 1)"), "6");
    assert_eq!(eval("(let ((x 1)) ((lambda () x)))"), "1");
    assert_eq!(eval("(define (f a) (let ((b 2)) (lambda (c) (+ a b c)))) ((f 1) 3)"), "6");
}

#[test]
fn let_in_tail_position_keeps_tail_calls() {
    let src = "(define (f n) (let ((m (* n 2))) (if (= m 0) 'zero (f (- n 1))))) (f 10000)";
    assert_eq!(eval_with_frame_limit(src, 100), "zero");
}

#[test]
fn let_syntax_errors() {
    assert!(eval("(let)").starts_with("error:"));
    assert!(eval("(let (x 1) x)").starts_with("error:"));
    assert!(eval("(let ((x)) x)").starts_with("error:"));
    assert!(eval("(let ((1 2)) 1)").starts_with("error:"));
    assert!(eval("(let ((x 1)))").starts_with("error:"));
}

#[test]
fn internal_defines_are_local_to_the_body() {
    assert_eq!(eval("(define (f) (define x 2) (* x 3)) (f)"), "6");
    assert_eq!(eval("(define (f) (define x 2) x) (f) x"), "error: x is not defined");
    assert_eq!(eval("(define (f) (define (sq x) (* x x)) (sq 5)) (f)"), "25");
    assert_eq!(eval("(let ((a 1)) (define b (+ a 1)) (* a b))"), "2");
    assert_eq!(eval("(define (f) (define x 1) (define y (+ x 1)) [x y]) (f)"), "[1 2]");
}

#[test]
fn internal_defines_can_refer_to_each_other() {
    let src = "(define (f n) (define (ev? n) (if (= n 0) true (od? (- n 1)))) (define (od? n) (if (= n 0) false (ev? (- n 1)))) (ev? n)) (f 10)";
    assert_eq!(eval(src), "true");
    assert_eq!(eval("(define (f) (define (g) (h)) (define (h) 42) (g)) (f)"), "42");
    assert_eq!(eval("(define (f) (define (loop i) (if (= i 3) 'done (loop (+ i 1)))) (loop 0)) (f)"), "done");
}

#[test]
fn set_mutates_locals_and_captured_variables() {
    assert_eq!(eval("(define (f) (define x 1) (set! x (+ x 1)) x) (f)"), "2");
    assert_eq!(eval("(let ((x 1)) (set! x 5) x)"), "5");
    let counter = "(define (counter) (let ((n 0)) (lambda () (set! n (+ n 1)) n)))";
    assert_eq!(eval(&format!("{counter} (define c (counter)) (c) (c) (c)")), "3");
    assert_eq!(eval(&format!("{counter} (define a (counter)) (define b (counter)) (a) (a) (b)")), "1");
    assert_eq!(eval("(define (f x) (lambda () (set! x (* x 2)) x)) (define g (f 3)) (g) (g)"), "12");
    assert_eq!(eval("(define (make) (define v 0) (define (inc!) (set! v (+ v 1))) (inc!) (inc!) v) (make)"), "2");
    assert_eq!(eval("(let ((n 0)) (define (bump!) (set! n (+ n 1))) (bump!) (bump!) n)"), "2");
}

#[test]
fn named_let_loops() {
    assert_eq!(eval("(let loop ((i 0) (acc 0)) (if (= i 5) acc (loop (+ i 1) (+ acc i))))"), "10");
    assert_eq!(eval_with_frame_limit("(let loop ((i 0)) (if (= i 10000) 'done (loop (+ i 1))))", 100), "done");
    assert_eq!(eval("(define (sum-to n) (let loop ((i n) (acc 0)) (if (= i 0) acc (loop (- i 1) (+ acc i))))) (sum-to 100)"), "5050");
    assert_eq!(eval("(+ 1 (let loop ((i 0)) (if (< i 3) (loop (+ i 1)) i)))"), "4");
}

#[test]
fn define_in_a_nested_expression_is_an_error() {
    assert!(eval("(define (f) (if true (define x 1)) x) (f)").starts_with("error:"));
}

#[test]
fn and_or_short_circuit_and_return_operands() {
    assert_eq!(eval("(and)"), "true");
    assert_eq!(eval("(and 1 2 3)"), "3");
    assert_eq!(eval("(and 1 nil 3)"), "nil");
    assert_eq!(eval("(and 1 false 3)"), "false");
    assert_eq!(eval("(or)"), "false");
    assert_eq!(eval("(or nil 2 3)"), "2");
    assert_eq!(eval("(or nil false)"), "false");
    assert_eq!(eval("(or 1 (undefined-thing))"), "1");
    assert_eq!(eval("(and nil (undefined-thing))"), "nil");
    assert_eq!(eval("(define x 0) (or false (begin (set! x 1) x) (set! x 2)) x"), "1");
    assert_eq!(eval("[(and 1 2) (or nil 3)]"), "[2 3]");
}

#[test]
fn and_or_keep_tail_calls() {
    assert_eq!(eval_with_frame_limit("(define (f n) (or (= n 0) (f (- n 1)))) (f 10000)", 100), "true");
    assert_eq!(
        eval_with_frame_limit("(define (f n) (and (> n -1) (if (= n 0) 'done (f (- n 1))))) (f 10000)", 100),
        "done"
    );
}

#[test]
fn cond_selects_the_first_true_clause() {
    assert_eq!(eval("(cond (false 1) (true 2) (else 3))"), "2");
    assert_eq!(eval("(cond (false 1) (else 3))"), "3");
    assert_eq!(eval("(cond (false 1))"), "nil");
    assert_eq!(eval("(cond ((+ 1 1)))"), "2");
    assert_eq!(eval("(cond (true 1 2 3))"), "3");
    let sign = "(define (sign n) (cond ((< n 0) 'neg) ((= n 0) 'zero) (else 'pos))) [(sign -5) (sign 0) (sign 3)]";
    assert_eq!(eval(sign), "[neg zero pos]");
    assert_eq!(eval("(+ 10 (cond (false 1) (else 5)))"), "15");
}

#[test]
fn cond_keeps_tail_calls() {
    let src = "(define (f n) (cond ((= n 0) 'done) ((= n -1) 'never) (else (f (- n 1))))) (f 10000)";
    assert_eq!(eval_with_frame_limit(src, 100), "done");
}

#[test]
fn cond_syntax_errors() {
    assert!(eval("(cond 1)").starts_with("error:"));
    assert!(eval("(cond (else 1) (true 2))").starts_with("error:"));
    assert!(eval("(cond ())").starts_with("error:"));
}

#[test]
fn when_and_unless_run_a_body_conditionally() {
    assert_eq!(eval("(when true 1 2)"), "2");
    assert_eq!(eval("(when false 1)"), "nil");
    assert_eq!(eval("(unless false 1 2)"), "2");
    assert_eq!(eval("(unless true 1)"), "nil");
    assert_eq!(eval("(define x 0) (when (= x 0) (set! x 5)) x"), "5");
    assert_eq!(eval_with_frame_limit("(define (f n) (when (> n -1) (if (= n 0) 'done (f (- n 1))))) (f 10000)", 100), "done");
}

#[test]
fn loop_and_recur_iterate_in_place() {
    assert_eq!(eval("(loop ((i 0) (acc 0)) (if (= i 5) acc (recur (+ i 1) (+ acc i))))"), "10");
    assert_eq!(eval_with_frame_limit("(loop ((i 0)) (if (= i 100000) 'done (recur (+ i 1))))", 10), "done");
    assert_eq!(eval("(+ 1 (loop ((i 0)) (if (< i 3) (recur (+ i 1)) i)))"), "4");
    assert_eq!(eval("(loop ((i 0)) (let ((j (* i 2))) (if (< j 10) (recur (+ i 1)) j)))"), "10");
    assert_eq!(eval("(define (f n) (loop ((i n)) (when (> i 0) (recur (- i 1))))) (f 3)"), "nil");
}

#[test]
fn recur_restarts_the_enclosing_function() {
    assert_eq!(eval("(define (count-down n) (if (= n 0) 'done (recur (- n 1)))) (count-down 1000)"), "done");
    assert_eq!(eval("(define (gcd a b) (if (= b 0) a (recur b (rem a b)))) (gcd 48 18)"), "6");
    assert_eq!(eval("((lambda (n acc) (if (= n 0) acc (recur (- n 1) (* acc n)))) 5 1)"), "120");
}

#[test]
fn recur_errors() {
    assert_eq!(eval("(loop ((i 0)) (+ 1 (recur i)))"), "error: recur: must be in tail position");
    assert_eq!(eval("(loop ((i 0)) (recur 1 2))"), "error: recur: expected 1 argument, got 2");
    assert_eq!(eval("(recur)"), "error: recur: not inside a function or loop");
}

#[test]
fn keywords_look_themselves_up_in_collections() {
    assert_eq!(eval("(:a {:a 1})"), "1");
    assert_eq!(eval("(:b {:a 1})"), "nil");
    assert_eq!(eval("(:b {:a 1} 0)"), "0");
    assert_eq!(eval("(:a nil)"), "nil");
    assert_eq!(eval("(:a 5)"), "nil");
    assert_eq!(eval("(:a #{:a :b})"), ":a");
    assert_eq!(eval("(:c #{:a :b} 'none)"), "none");
    assert_eq!(eval("(define (name-of p) (:name p)) (name-of {:name \"Ada\"})"), "\"Ada\"");
    assert_eq!(eval("(:a)"), "error: :a expects 1 to 2 arguments, got 0");
}

#[test]
fn collections_are_lookup_functions() {
    assert_eq!(eval("({:a 1} :a)"), "1");
    assert_eq!(eval("({:a 1} :b 'none)"), "none");
    assert_eq!(eval("({\"k\" 2} \"k\")"), "2");
    assert_eq!(eval("([10 20] 1)"), "20");
    assert_eq!(eval("([10 20] 5 'd)"), "d");
    assert_eq!(eval("([10 20] 5)"), "error: index 5 is out of bounds for a vector of length 2");
    assert_eq!(eval("([10 20] :x)"), "error: vector index must be an integer, got :x");
    assert_eq!(eval("(#{1 2} 1)"), "1");
    assert_eq!(eval("(#{1 2} 3)"), "nil");
}

#[test]
fn error_raises_a_catchable_map() {
    assert_eq!(eval("(error \"boom\")"), "error: boom");
    assert_eq!(eval("(try (error \"boom\") (catch e (:message e)))"), "\"boom\"");
    assert_eq!(eval("(try (error \"boom\" {:code 7}) (catch e (:data e)))"), "{:code 7}");
    assert_eq!(eval("(try (error \"boom\") (catch e (:kind e)))"), ":user");
}

#[test]
fn runtime_errors_are_catchable() {
    assert_eq!(eval("(try (+ 1 \"a\") (catch e (:kind e)))"), ":type-error");
    assert_eq!(eval("(try (undefined-thing) (catch e (:message e)))"), "\"undefined-thing is not defined\"");
    assert_eq!(eval("(try (quot 1 0) (catch e (:kind e)))"), ":arithmetic");
    assert_eq!(eval("(try ((lambda (x) x)) (catch e (:kind e)))"), ":arity-error");
}

#[test]
fn raise_throws_any_value() {
    assert_eq!(eval("(try (raise 42) (catch e (+ e 1)))"), "43");
    assert_eq!(eval("(raise :oops)"), "error: :oops");
    assert_eq!(eval("(try (raise {:message \"custom\"}) (catch e (:message e)))"), "\"custom\"");
}

#[test]
fn try_returns_the_body_value_when_nothing_is_raised() {
    assert_eq!(eval("(try 1 2 3 (catch e 'caught))"), "3");
    assert_eq!(eval("(try (+ 1 2))"), "3");
    assert_eq!(eval("[(try (raise 1) (catch e e)) (try 2 (catch e e))]"), "[1 2]");
}

#[test]
fn errors_unwind_through_nested_calls_and_scopes() {
    assert_eq!(eval("(define (f) (g)) (define (g) (error \"deep\")) (try (f) (catch e (:message e)))"), "\"deep\"");
    let safe_div = "(define (safe-div a b) (try (if (= b 0) (error \"zero\") (/ a b)) (catch e nil))) [(safe-div 1 2) (safe-div 1 0)]";
    assert_eq!(eval(safe_div), "[0.5 nil]");
    assert_eq!(eval("(try (try (raise 1) (catch e (raise (+ e 1)))) (catch e (* e 10)))"), "20");
    assert_eq!(eval("(let ((x 1)) (try (raise 2) (catch e (+ x e))))"), "3");
    assert_eq!(eval("(define (f) (try (raise 1) (catch e e))) (define (g n) (if (= n 0) (f) (g (- n 1)))) (g 50)"), "1");
    assert_eq!(eval("(try (raise 1) (catch e e)) (+ 1 2)"), "3");
}

#[test]
fn finally_runs_on_every_path() {
    assert_eq!(eval("(define log nil) (try 1 (finally (set! log 'ran))) log"), "ran");
    assert_eq!(eval("(define log nil) (try (raise 1) (catch e e) (finally (set! log 'ran))) log"), "ran");
    assert_eq!(eval("(define log nil) (try (try (raise 1) (finally (set! log 'ran))) (catch e [e log]))"), "[1 ran]");
    assert_eq!(eval("(try (raise 1) (finally 2))"), "error: 1");
    assert_eq!(eval("(try 'body (finally 'ignored))"), "body");
    let nested = "(define log nil) (try (try (raise 1) (catch e (raise 2)) (finally (set! log 'ran))) (catch e [e log]))";
    assert_eq!(eval(nested), "[2 ran]");
}

#[test]
fn try_syntax_errors() {
    assert!(eval("(try 1 (catch))").starts_with("error:"));
    assert!(eval("(try 1 (catch 1 2))").starts_with("error:"));
    assert!(eval("(try 1 (finally 2) (catch e 3))").starts_with("error:"));
    assert!(eval("(try 1 (catch e 2) (catch e 3))").starts_with("error:"));
    assert!(eval("(try (catch e 1) 2)").starts_with("error:"));
}

#[test]
fn call_ec_escapes_early() {
    assert_eq!(eval("(call/ec (lambda (k) 1 (k 2) 3))"), "2");
    assert_eq!(eval("(call/ec (lambda (k) 1 2))"), "2");
    assert_eq!(eval("(+ 1 (call/ec (lambda (k) (+ 10 (k 5)))))"), "6");
    assert_eq!(eval("(define (f k) (k 'escaped) 'not-here) (call/ec (lambda (k) (f k) 'nope))"), "escaped");
    assert_eq!(eval("(call/ec (lambda (k) (k)))"), "nil");
    assert_eq!(eval("(let ((x 1)) (call/ec (lambda (k) (k (+ x 1)))))"), "2");
}

#[test]
fn escapes_pass_through_catch_but_run_finally() {
    assert_eq!(eval("(call/ec (lambda (k) (try (k 'out) (catch e 'caught))))"), "out");
    assert_eq!(eval("(define log nil) (call/ec (lambda (k) (try (k 'out) (finally (set! log 'cleaned))))) log"), "cleaned");
    assert_eq!(eval("(try (call/ec (lambda (k) (raise 'inner))) (catch e e))"), "inner");
}

#[test]
fn escape_outside_its_extent_is_an_error() {
    let src = "(define saved nil) (call/ec (lambda (k) (set! saved k))) (saved 1)";
    assert_eq!(eval(src), "error: escape continuation called outside its extent");
    assert_eq!(eval("(call/ec (lambda (k) (k 1 2)))"), "error: escape continuation expects 0 to 1 arguments, got 2");
    assert!(eval("(call/ec)").starts_with("error:"));
}

#[test]
fn first_rest_and_cons_form_the_sequence_protocol() {
    assert_eq!(eval("(first '(1 2 3))"), "1");
    assert_eq!(eval("(first [1 2])"), "1");
    assert_eq!(eval("(first [])"), "nil");
    assert_eq!(eval("(first nil)"), "nil");
    assert_eq!(eval("(first {:a 1})"), "[:a 1]");
    assert_eq!(eval("(rest '(1 2 3))"), "(2 3)");
    assert_eq!(eval("(rest [1 2 3])"), "(2 3)");
    assert_eq!(eval("(rest [])"), "()");
    assert_eq!(eval("(rest nil)"), "()");
    assert_eq!(eval("(rest {:a 1, :b 2})"), "([:b 2])");
    assert_eq!(eval("(cons 1 '(2))"), "(1 2)");
    assert_eq!(eval("(cons 1 nil)"), "(1)");
    assert_eq!(eval("(cons 1 [2 3])"), "(1 2 3)");
    assert_eq!(eval("(cons 1 2)"), "error: cons expects a collection, got 2");
    assert_eq!(eval("(define (sum xs) (if (empty? xs) 0 (+ (first xs) (sum (rest xs))))) (sum [1 2 3 4])"), "10");
}

#[test]
fn empty_count_and_last_work_on_every_collection() {
    assert_eq!(eval("[(empty? []) (empty? '()) (empty? nil) (empty? [1]) (empty? {}) (empty? \"\")]"), "[true true true false true true]");
    assert_eq!(eval("(empty? 5)"), "error: empty? expects a collection, got 5");
    assert_eq!(eval("[(count [1 2 3]) (count '(1 2)) (count {:a 1}) (count \"héllo\") (count nil) (count #{1 2})]"), "[3 2 1 5 0 2]");
    assert_eq!(eval("[(last [1 2 3]) (last '(1 2 3)) (last []) (last nil)]"), "[3 3 nil nil]");
}

#[test]
fn nth_indexes_lists_vectors_and_strings() {
    assert_eq!(eval("(nth [10 20 30] 1)"), "20");
    assert_eq!(eval("(nth '(10 20 30) 2)"), "30");
    assert_eq!(eval("(nth \"abc\" 1)"), "\"b\"");
    assert_eq!(eval("(nth [1] 5)"), "error: index 5 is out of bounds for a vector of length 1");
    assert_eq!(eval("(nth [1] 5 'd)"), "d");
    assert_eq!(eval("(nth '(1) -1)"), "error: index -1 is out of bounds for a list of length 1");
}

#[test]
fn conj_adds_where_each_collection_grows() {
    assert_eq!(eval("(conj [1 2] 3)"), "[1 2 3]");
    assert_eq!(eval("(conj [1] 2 3)"), "[1 2 3]");
    assert_eq!(eval("(conj '(2 3) 1)"), "(1 2 3)");
    assert_eq!(eval("(conj #{1} 2)"), "#{1 2}");
    assert_eq!(eval("(conj {:a 1} [:b 2])"), "{:a 1, :b 2}");
    assert_eq!(eval("(conj {:a 1} {:b 2})"), "{:a 1, :b 2}");
    assert_eq!(eval("(conj nil 1)"), "(1)");
    assert_eq!(eval("(conj {} 1)"), "error: conj onto a map expects a [key value] pair or a map, got 1");
    assert_eq!(eval("(conj 5 1)"), "error: conj expects a collection, got 5");
}

#[test]
fn collection_constructors() {
    assert_eq!(eval("(list 1 2 3)"), "(1 2 3)");
    assert_eq!(eval("(list)"), "()");
    assert_eq!(eval("(vector 1 2)"), "[1 2]");
    assert_eq!(eval("[(vec '(1 2)) (vec #{1}) (vec nil) (vec {:a 1})]"), "[[1 2] [1] [] [[:a 1]]]");
    assert_eq!(eval("(hash-map :a 1 :b 2)"), "{:a 1, :b 2}");
    assert_eq!(eval("(hash-map :a)"), "error: hash-map expects an even number of arguments, got 1");
    assert_eq!(eval("(hash-set 1 2 1)"), "#{1 2}");
    assert_eq!(eval("(set [1 1 2])"), "#{1 2}");
    assert_eq!(eval("[(range 4) (range 1 4) (range 0 10 3) (range 5 0 -2) (range 0)]"), "[[0 1 2 3] [1 2 3] [0 3 6 9] [5 3 1] []]");
    assert_eq!(eval("(range 0 1 0.25)"), "[0.0 0.25 0.5 0.75]");
    assert_eq!(eval("(range 0 1 0)"), "error: range step must not be zero");
    assert_eq!(eval("[(reverse [1 2 3]) (reverse '(1 2)) (reverse nil)]"), "[[3 2 1] (2 1) ()]");
}

#[test]
fn type_predicates() {
    assert_eq!(eval("[(nil? nil) (nil? false)]"), "[true false]");
    assert_eq!(eval("[(integer? 1) (integer? 1.0) (float? 1.0) (number? 1) (number? \"1\")]"), "[true false true true false]");
    assert_eq!(eval("[(string? \"s\") (symbol? 'a) (keyword? :a) (boolean? false)]"), "[true true true true]");
    assert_eq!(eval("[(list? '(1)) (list? [1]) (vector? [1]) (map? {}) (set? #{})]"), "[true false true true true]");
    assert_eq!(eval("[(procedure? +) (procedure? (lambda () 1)) (procedure? 1)]"), "[true true false]");
    assert_eq!(eval("[(coll? [1]) (coll? '()) (coll? {}) (coll? \"s\") (coll? 1)]"), "[true true true false false]");
}

#[test]
fn get_reads_any_indexed_or_keyed_collection() {
    assert_eq!(eval("[(get {:a 1} :a) (get {:a 1} :b) (get {:a 1} :b 0)]"), "[1 nil 0]");
    assert_eq!(eval("[(get [1 2] 1) (get [1 2] 5) (get [1 2] -1 'd)]"), "[2 nil d]");
    assert_eq!(eval("[(get nil :a) (get #{1} 1) (get \"abc\" 1) (get 5 1)]"), "[nil 1 \"b\" nil]");
    assert_eq!(eval("[(find {:a 1} :a) (find {:a 1} :b)]"), "[[:a 1] nil]");
}

#[test]
fn assoc_and_dissoc_update_maps_and_vectors() {
    assert_eq!(eval("(assoc {:a 1} :b 2)"), "{:a 1, :b 2}");
    assert_eq!(eval("(assoc {} :a 1 :b 2)"), "{:a 1, :b 2}");
    assert_eq!(eval("(assoc nil :a 1)"), "{:a 1}");
    assert_eq!(eval("[(assoc [1 2] 0 9) (assoc [1 2] 2 3)]"), "[[9 2] [1 2 3]]");
    assert_eq!(eval("(assoc [1 2] 5 0)"), "error: index 5 is out of bounds for a vector of length 2");
    assert_eq!(eval("(assoc {} :a)"), "error: assoc expects an even number of key and value arguments, got 1");
    assert_eq!(eval("(assoc 5 :a 1)"), "error: assoc expects a map, vector, or table, got 5");
    assert_eq!(eval("[(dissoc {:a 1, :b 2} :a) (dissoc {:a 1} :zz) (dissoc {:a 1, :b 2, :c 3} :a :c) (dissoc nil :a)]"), "[{:b 2} {:a 1} {:b 2} nil]");
}

#[test]
fn map_queries_and_combinations() {
    assert_eq!(eval("[(keys {:a 1, :b 2}) (vals {:a 1, :b 2}) (keys {}) (keys nil)]"), "[[:a :b] [1 2] [] []]");
    assert_eq!(
        eval("[(contains? {:a 1} :a) (contains? {:a 1} :b) (contains? #{1} 1) (contains? [1 2] 1) (contains? [1 2] 2) (contains? nil :a)]"),
        "[true false true true false false]"
    );
    assert_eq!(eval("(contains? 5 1)"), "error: contains? expects a collection, got 5");
    assert_eq!(eval("[(merge {:a 1} {:b 2} {:a 3}) (merge) (merge {:a 1} nil)]"), "[{:a 3, :b 2} {} {:a 1}]");
    assert_eq!(eval("[(disj #{1 2 3} 1 3) (disj #{1} 5)]"), "[#{2} #{1}]");
    assert_eq!(eval("[(select-keys {:a 1, :b 2, :c 3} [:a :c]) (select-keys {:a 1} [:zz])]"), "[{:a 1, :c 3} {}]");
    assert_eq!(eval("(zipmap [:a :b] [1 2])"), "{:a 1, :b 2}");
    assert_eq!(eval("[(subvec [1 2 3 4] 1 3) (subvec [1 2 3] 1) (subvec [1 2] 2)]"), "[[2 3] [2 3] []]");
    assert_eq!(eval("(subvec [1 2] 3)"), "error: subvec range 3 to 2 is out of bounds for a vector of length 2");
}

#[test]
fn apply_spreads_a_collection_as_arguments() {
    assert_eq!(eval("(apply + [1 2 3])"), "6");
    assert_eq!(eval("(apply + 1 2 [3 4])"), "10");
    assert_eq!(eval("(apply list '(1 2))"), "(1 2)");
    assert_eq!(eval("(apply + nil)"), "0");
    assert_eq!(eval("(apply (lambda (a b) (- a b)) [10 3])"), "7");
    assert_eq!(eval("(apply + 1)"), "error: apply expects a collection as its last argument, got 1");
}

#[test]
fn map_filter_and_reduce() {
    assert_eq!(eval("(map (lambda (x) (* x 2)) [1 2 3])"), "[2 4 6]");
    assert_eq!(eval("(map (lambda (x) (* x 2)) '(1 2 3))"), "(2 4 6)");
    assert_eq!(eval("(map + [1 2] [10 20])"), "[11 22]");
    assert_eq!(eval("(map + [1 2 3] [10 20])"), "[11 22]");
    assert_eq!(eval("(map first {:a 1})"), "[:a]");
    assert_eq!(eval("(map (lambda (x) x) nil)"), "[]");
    assert_eq!(eval("(filter (lambda (x) (> x 1)) [1 2 3])"), "[2 3]");
    assert_eq!(eval("(filter integer? '(1 \"a\" 2))"), "(1 2)");
    assert_eq!(eval("(remove integer? [1 \"a\" 2])"), "[\"a\"]");
    assert_eq!(eval("(reduce + 0 [1 2 3])"), "6");
    assert_eq!(eval("(reduce + [1 2 3])"), "6");
    assert_eq!(eval("(reduce + [])"), "0");
    assert_eq!(eval("(reduce (lambda (acc x) (conj acc x)) [] '(1 2))"), "[1 2]");
    assert_eq!(eval("(reduce + 5 nil)"), "5");
    assert_eq!(eval("(define (inc x) (+ x 1)) (map inc (map inc [1]))"), "[3]");
}

#[test]
fn for_each_some_and_every() {
    assert_eq!(eval("(define total 0) (for-each (lambda (x) (set! total (+ total x))) [1 2 3]) total"), "6");
    assert_eq!(eval("(for-each (lambda (x) x) [1])"), "nil");
    assert_eq!(eval("(some (lambda (x) (and (> x 1) x)) [1 2 3])"), "2");
    assert_eq!(eval("(some (lambda (x) (> x 5)) [1 2])"), "nil");
    assert_eq!(eval("[(every? integer? [1 2]) (every? integer? [1 \"a\"]) (every? integer? [])]"), "[true false true]");
}

#[test]
fn errors_and_escapes_cross_native_callbacks_cleanly() {
    assert_eq!(eval("(try (map (lambda (x) (error \"bad\")) [1]) (catch e (:message e)))"), "\"bad\"");
    assert_eq!(eval("(call/ec (lambda (k) (map (lambda (x) (k x)) [7 8])))"), "7");
    assert_eq!(eval("(define (f) (map (lambda (x) (raise x)) [1])) (try (f) (catch e e))"), "1");
    assert_eq!(eval("(map 5 [1])"), "error: 5 is not a function");
    assert_eq!(eval("(try (map (lambda (x) (error \"x\")) [1]) (catch e nil)) (+ 1 2)"), "3");
    assert_eq!(eval("(define (f) (try (raise 1) (catch e e))) (map (lambda (x) (f)) [1 2])"), "[1 1]");
}

#[test]
fn sort_orders_values_naturally_or_by_a_comparator() {
    assert_eq!(eval("[(sort [3 1 2]) (sort '(3 1 2)) (sort [1.5 1 2]) (sort []) (sort nil)]"), "[[1 2 3] (1 2 3) [1 1.5 2] [] []]");
    assert_eq!(eval("[(sort [\"b\" \"a\"]) (sort [:b :a]) (sort ['b 'a]) (sort [true false])]"), "[[\"a\" \"b\"] [:a :b] [a b] [false true]]");
    assert_eq!(eval("(sort [[1 2] [1 1] [0 9]])"), "[[0 9] [1 1] [1 2]]");
    assert_eq!(eval("(sort > [1 3 2])"), "[3 2 1]");
    assert_eq!(eval("(sort (lambda (a b) (- b a)) [1 3 2])"), "[3 2 1]");
    assert_eq!(eval("(sort [1 \"a\"])"), "error: sort cannot compare 1 and \"a\"");
    assert_eq!(eval("(sort (lambda (a b) (< (:n a) (:n b))) [{:n 2 :k 1} {:n 1} {:n 2 :k 2}])"), "[{:n 1} {:n 2, :k 1} {:n 2, :k 2}]");
}

#[test]
fn sort_by_and_compare() {
    assert_eq!(eval("(sort-by :age [{:age 30} {:age 20}])"), "[{:age 20} {:age 30}]");
    assert_eq!(eval("(sort-by count [\"ccc\" \"a\" \"bb\"])"), "[\"a\" \"bb\" \"ccc\"]");
    assert_eq!(eval("(sort-by :age > [{:age 30} {:age 20}])"), "[{:age 30} {:age 20}]");
    assert_eq!(eval("(sort-by - '(1 3 2))"), "(3 2 1)");
    assert_eq!(
        eval("[(compare 1 2) (compare 2 1) (compare 1 1) (compare \"a\" \"b\") (compare :a :b) (compare [1 2] [1 3]) (compare nil 1) (compare 1 1.0)]"),
        "[-1 1 0 -1 -1 -1 -1 0]"
    );
    assert_eq!(eval("(compare 1 :a)"), "error: compare cannot compare 1 and :a");
}

#[test]
fn nested_updates() {
    assert_eq!(eval("(update {:a 1} :a + 10)"), "{:a 11}");
    assert_eq!(eval("(update {:a 1} :b (lambda (x) (if x 1 0)))"), "{:a 1, :b 0}");
    assert_eq!(eval("(update [1 2] 0 - 5)"), "[-4 2]");
    assert_eq!(eval("[(get-in {:a {:b 1}} [:a :b]) (get-in {:a {:b 1}} [:a :c]) (get-in {:a {:b 1}} [:a :c] 0)]"), "[1 nil 0]");
    assert_eq!(eval("[(get-in {:a [10 20]} [:a 1]) (get-in nil [:a]) (get-in {:a 1} [])]"), "[20 nil {:a 1}]");
    assert_eq!(eval("[(assoc-in {} [:a :b] 1) (assoc-in {:a {:b 1}} [:a :c] 2) (assoc-in {:a [1 2]} [:a 0] 9)]"), "[{:a {:b 1}} {:a {:b 1, :c 2}} {:a [9 2]}]");
    assert_eq!(eval("[(update-in {:a {:b 1}} [:a :b] + 1) (update-in {} [:a :b] (lambda (x) (if x x 'new)))]"), "[{:a {:b 2}} {:a {:b new}}]");
    assert_eq!(eval("(assoc-in {} [] 1)"), "error: assoc-in expects at least one key");
}

#[test]
fn take_drop_and_other_sequence_transformations() {
    assert_eq!(eval("[(take 2 [1 2 3]) (take 5 [1 2]) (take 0 [1]) (drop 1 [1 2 3]) (drop 5 [1 2]) (take 2 '(1 2 3))]"), "[[1 2] [1 2] [] [2 3] [] (1 2)]");
    assert_eq!(eval("[(take-while even? [2 4 5 6]) (drop-while even? [2 4 5 6])]"), "[[2 4] [5 6]]");
    assert_eq!(eval("[(concat [1 2] '(3) nil [4]) (concat) (concat '(1) [2])]"), "[[1 2 3 4] [] (1 2)]");
    assert_eq!(eval("[(into [] '(1 2)) (into '() [1 2]) (into #{} [1 1 2]) (into {} [[:a 1]]) (into {:a 1} {:b 2})]"), "[[1 2] (2 1) #{1 2} {:a 1} {:a 1, :b 2}]");
    assert_eq!(eval("[(distinct [1 2 1 3 2]) (distinct '(1 1))]"), "[[1 2 3] (1)]");
    assert_eq!(eval("(frequencies [:a :b :a])"), "{:a 2, :b 1}");
    assert_eq!(eval("(group-by even? [1 2 3 4])"), "{false [1 3], true [2 4]}");
    assert_eq!(eval("(group-by :k [{:k 1 :v :a} {:k 2} {:k 1 :v :b}])"), "{1 [{:k 1, :v :a} {:k 1, :v :b}], 2 [{:k 2}]}");
    assert_eq!(eval("[(partition 2 [1 2 3 4 5]) (partition 2 1 [1 2 3])]"), "[[[1 2] [3 4]] [[1 2] [2 3]]]");
    assert_eq!(eval("[(interleave [1 2] [:a :b]) (flatten [1 [2 [3]] 4]) (second [1 2 3]) (second [1])]"), "[[1 :a 2 :b] [1 2 3 4] 2 nil]");
    assert_eq!(eval("(take 1 5)"), "error: take expects a collection, got 5");
    assert_eq!(eval("(take -1 [1])"), "error: take expects a non-negative count, got -1");
}

#[test]
fn numeric_builtins() {
    assert_eq!(eval("[(max 1 3 2) (min 1 3 2) (max 1 2.5) (abs -3) (abs -2.5) (inc 1) (dec 1)]"), "[3 1 2.5 3 2.5 2 0]");
    assert_eq!(eval("[(even? 2) (odd? 2) (zero? 0) (zero? 0.0) (pos? 1) (neg? -1) (pos? 0)]"), "[true false true true true true false]");
    assert_eq!(eval("[(floor 2.7) (ceil 2.1) (round 2.5) (round 3.5) (round -2.5) (round 2.4) (truncate -2.7) (floor 3)]"), "[2 3 2 4 -2 2 -2 3]");
    assert_eq!(eval("[(sqrt 16) (sqrt 2.25) (pow 2 10) (pow 2 0.5) (pow 2.0 2) (pow 2 -1)]"), "[4.0 1.5 1024 1.4142135623730951 4.0 0.5]");
    assert_eq!(eval("[(exp 0) (log 1) (log 100 10) (log 8 2)]"), "[1.0 0.0 2.0 3.0]");
    assert_eq!(eval("[(int 2.7) (int -2.7) (int 3) (float 2) (float 2.5)]"), "[2 -2 3 2.0 2.5]");
    assert_eq!(eval("(floor +inf.0)"), "error: cannot convert +inf.0 to an integer");
    assert_eq!(eval("(max 1 'a)"), "error: max expects numbers, got a");
    assert_eq!(eval("(even? 1.5)"), "error: even? expects an integer, got 1.5");
    assert_eq!(eval("(pow 2 100)"), "error: integer overflow");
}

#[test]
fn str_joins_the_display_forms_of_its_arguments() {
    assert_eq!(eval("(str \"a\" 1 :k nil 'sym [1 \"b\"] 2.5)"), "\"a1:ksym[1 b]2.5\"");
    assert_eq!(eval("(str)"), "\"\"");
    assert_eq!(eval("(str {:a \"x\"})"), "\"{:a x}\"");
}

#[test]
fn string_functions() {
    assert_eq!(eval("[(substring \"hello\" 1 3) (substring \"hello\" 2) (substring \"héllo\" 1 2)]"), "[\"el\" \"llo\" \"é\"]");
    assert_eq!(eval("(substring \"hi\" 3)"), "error: substring range 3 to 2 is out of bounds for a string of length 2");
    assert_eq!(eval("[(string-split \"a,b,,c\" \",\") (string-split \"a b  c\") (string-split \"\" \",\")]"), "[[\"a\" \"b\" \"\" \"c\"] [\"a\" \"b\" \"c\"] [\"\"]]");
    assert_eq!(eval("[(string-join \", \" [1 \"a\" :b]) (string-join [\"a\" \"b\"]) (string-join \"-\" [])]"), "[\"1, a, :b\" \"ab\" \"\"]");
    assert_eq!(eval("[(string-upcase \"abc\") (string-downcase \"ABC\") (string-trim \"  x \")]"), "[\"ABC\" \"abc\" \"x\"]");
    assert_eq!(eval("[(string-contains? \"hello\" \"ell\") (string-starts-with? \"hello\" \"he\") (string-ends-with? \"hello\" \"lo\") (string-contains? \"hello\" \"z\")]"), "[true true true false]");
    assert_eq!(eval("[(string-replace \"a-b-c\" \"-\" \"+\") (string-index-of \"hello\" \"l\") (string-index-of \"hello\" \"z\")]"), "[\"a+b+c\" 2 nil]");
    assert_eq!(eval("[(string->number \"42\") (string->number \"2.5\") (string->number \"x\") (string->number \"1e3\") (string->number \" 7 \")]"), "[42 2.5 nil 1000.0 7]");
    assert_eq!(eval("[(number->string 42) (number->string 2.5)]"), "[\"42\" \"2.5\"]");
    assert_eq!(eval("[(name :k) (name 'sym) (name \"s\") (keyword \"k\") (symbol \"s\") (keyword 'k) (keyword :k)]"), "[\"k\" \"sym\" \"s\" :k s :k :k]");
    assert_eq!(eval("(string-upcase 5)"), "error: string-upcase expects a string, got 5");
    assert_eq!(eval("(string-repeat \"ab\" 3)"), "\"ababab\"");
}

#[test]
fn format_fills_brace_placeholders() {
    assert_eq!(eval("(format \"{} sold {} units\" \"Oslo\" 12)"), "\"Oslo sold 12 units\"");
    assert_eq!(eval("(format \"{1} before {0}, then {}\" \"a\" \"b\")"), "\"b before a, then a\"");
    assert_eq!(
        eval("[(format \"{:.2}\" 3.14159) (format \"{:.2}\" 2) (format \"{:.0}\" 2.71) (format \"{:,}\" 1234567) (format \"{:,.2}\" -1234567.891)]"),
        "[\"3.14\" \"2.00\" \"3\" \"1,234,567\" \"-1,234,567.89\"]"
    );
    assert_eq!(
        eval("(format \"{:>6}|{:<6}|{:^6}|{:5}|{:5}\" \"ab\" \"cd\" \"ef\" 42 \"ab\")"),
        "\"    ab|cd    |  ef  |   42|ab   \""
    );
    assert_eq!(
        eval("[(format \"{:06.2}\" 3.14159) (format \"{:06.1}\" -3.14159) (format \"{:*^7}\" \"hi\") (format \"{:.2}\" \"text\")]"),
        "[\"003.14\" \"-003.1\" \"**hi***\" \"te\"]"
    );
    assert_eq!(
        eval("[(format \"{:.1%}\" 0.256) (format \"{:%}\" 0.256) (format \"{:e}\" 1234.5) (format \"{:.2e}\" 1234.5)]"),
        "[\"25.6%\" \"26%\" \"1.2345e3\" \"1.23e3\"]"
    );
    assert_eq!(
        eval("[(format \"{:?} and {}\" \"q\" \"q\") (format \"{:?}|{}\" nil nil) (format \"{{}} {}\" [1 \"a\"])]"),
        "[\"\\\"q\\\" and q\" \"nil|\" \"{} [1 a]\"]"
    );
    assert_eq!(eval("(format \"{:>3}|\" nil)"), "\"   |\"");
    assert_eq!(eval("(format \"{} {}\" 1)"), "error: format expects 2 values for \"{} {}\", got 1");
    assert_eq!(eval("(format \"{:z}\" 1)"), "error: format does not understand \"{:z}\"");
    assert_eq!(eval("(format \"a { b\" 1)"), "error: format found an unmatched { in \"a { b\"");
    assert_eq!(eval("(format \"a } b\")"), "error: format found an unmatched } in \"a } b\"");
    assert_eq!(eval("(format 5)"), "error: format expects a string template, got 5");
}

#[test]
fn format_and_string_functions_broadcast_over_arrays() {
    assert_eq!(eval("(format \"{}: {:.1}\" #[\"a\" \"b\"] #[1 2.26])"), "#[\"a: 1.0\" \"b: 2.3\"]");
    assert_eq!(eval("(format \"{}-{}\" \"x\" #[1 nil])"), "#[\"x-1\" \"x-\"]");
    assert_eq!(eval("(format \"{}\" #[1 2] #[1 2 3])"), "error: array lengths differ: 2 and 3");
    assert_eq!(
        eval("[(string-upcase #[\"a\" nil \"c\"]) (string-length #[\"ab\" \"c\"]) (string-length \"héllo\")]"),
        "[#[\"A\" nil \"C\"] #[2 1] 5]"
    );
    assert_eq!(
        eval("[(string-contains? #[\"apple\" \"pear\"] \"pp\") (string->number #[\"1\" \"x\" \"2.5\"]) (string-index-of #[\"hello\" \"x\"] \"l\")]"),
        "[#[true false] #[1.0 nil 2.5] #[2 nil]]"
    );
    assert_eq!(
        eval("[(substring #[\"hello\" \"hi\"] 0 2) (string-replace #[\"a-b\" \"c\"] \"-\" \"+\") (string-split #[\"a,b\" nil] \",\") (number->string #[1 2.5])]"),
        "[#[\"he\" \"hi\"] #[\"a+b\" \"c\"] [[\"a\" \"b\"] nil] #[\"1.0\" \"2.5\"]]"
    );
    assert_eq!(eval("(string-upcase #[1 2])"), "error: string-upcase expects a string, got 1");
}

#[test]
fn dates_are_values_with_a_literal_and_calendar_functions() {
    let d = "(define d #2024-03-15)";
    assert_eq!(eval("#2024-03-15"), "#2024-03-15");
    assert_eq!(eval("[(str #2024-03-15) (format \"{:?} {}\" #2024-03-15 #2024-03-15) (date? #2024-03-15) (date? \"2024-03-15\")]"), "[\"2024-03-15\" \"#2024-03-15 2024-03-15\" true false]");
    assert_eq!(
        eval("[(date \"2024-03-15\") (date 2024 3 15) (date \"15/03/2024\" \"dd/MM/yyyy\") (date \"March 5, 2024\" \"MMMM d, yyyy\") (date \"5 jan 99\" \"d MMM yy\")]"),
        "[#2024-03-15 #2024-03-15 #2024-03-15 #2024-03-05 #1999-01-05]"
    );
    assert_eq!(eval(&format!("{d} [(year d) (month d) (day d) (weekday d) (day-of-year d) (quarter d)]")), "[2024 3 15 5 75 1]");
    assert_eq!(
        eval(&format!("{d} [(+ d 20) (- d 15) (- #2024-04-01 d) (inc d) (dec d) (< d #2024-04-01) (= d (date \"2024-03-15\")) (max d #2023-01-01) (min #2023-01-01 d)]")),
        "[#2024-04-04 #2024-02-29 17 #2024-03-16 #2024-03-14 true true #2024-03-15 #2023-01-01]"
    );
    assert_eq!(
        eval(&format!("{d} [(date-format d \"dd MMM yyyy\") (date-format d \"EEEE, MMMM d\") (date-format d \"yy/M/d (EEE)\")]")),
        "[\"15 Mar 2024\" \"Friday, March 15\" \"24/3/15 (Fri)\"]"
    );
    assert_eq!(
        eval(&format!("{d} [(date-trunc d :month) (date-trunc d :week) (date-trunc d :quarter) (date-trunc d :year)]")),
        "[#2024-03-01 #2024-03-11 #2024-01-01 #2024-01-01]"
    );
    assert_eq!(
        eval(&format!("{d} [(add-months #2024-01-31 1) (add-months d -3) (add-years #2024-02-29 1) (days-in-month d) (leap-year? d) (leap-year? 1900)]")),
        "[#2024-02-29 #2023-12-15 #2025-02-28 31 true false]"
    );
    assert_eq!(eval("(date-range #2024-01-30 #2024-02-02)"), "#[#2024-01-30 #2024-01-31 #2024-02-01 #2024-02-02]");
    assert_eq!(eval("(date-range #2024-01-31 #2024-04-30 :month)"), "#[#2024-01-31 #2024-02-29 #2024-03-31 #2024-04-30]");
    assert_eq!(eval("[(date-range #2024-01-01 #2024-01-20 :week) (count (date-range #2024-01-02 #2024-01-01))]"), "[#[#2024-01-01 #2024-01-08 #2024-01-15] 0]");
    assert_eq!(eval("(sort [#2024-03-15 #2023-12-31 #2024-01-01])"), "[#2023-12-31 #2024-01-01 #2024-03-15]");
    assert_eq!(eval("[(get {#2024-01-01 \"new year\"} #2024-01-01) (contains? #{#2024-01-01} (date 2024 1 1))]"), "[\"new year\" true]");
    assert_eq!(eval("(date? (today))"), "true");
    assert_eq!(eval("(date \"2024-02-30\")"), "error: date cannot read \"2024-02-30\" as a date");
    assert_eq!(eval("(date \"15/03/2024\")"), "error: date cannot read \"15/03/2024\" as a date");
    assert_eq!(eval("(date \"2024-03-15x\" \"yyyy-MM-dd\")"), "error: date cannot read \"2024-03-15x\" with the pattern \"yyyy-MM-dd\"");
    assert_eq!(eval("(date 2024 2 30)"), "error: 2024-02-30 is not a valid date");
    assert_eq!(eval("(date-trunc #2024-03-15 :hour)"), "error: date-trunc expects :week, :month, :quarter, or :year, got :hour");
    assert_eq!(eval("(* #2024-03-15 2)"), "error: * expects numbers, got #2024-03-15");
    assert_eq!(eval("(+ #2024-03-15 #2024-03-15)"), "error: + expects numbers, got #2024-03-15");
    assert!(eval("#2024-13-01").contains("invalid date literal #2024-13-01"));
}

#[test]
fn date_functions_broadcast_over_date_arrays() {
    let ds = "(define ds #[#2024-01-01 #2024-02-15 nil])";
    assert_eq!(
        eval(&format!("{ds} [(array-type ds) (year ds) (+ ds 1) (- #2024-03-01 ds) (< ds #2024-02-01) (date-format ds \"MMM\")]")),
        "[:date #[2024 2024 nil] #[#2024-01-02 #2024-02-16 nil] #[60 15 nil] #[true false nil] #[\"Jan\" \"Feb\" nil]]"
    );
    assert_eq!(
        eval(&format!("{ds} [(date-trunc ds :month) (add-months ds 1) (month (date #[\"2024-05-01\" nil \"2023-01-09\"])) (day (date #[\"01.02.2024\"] \"dd.MM.yyyy\"))]")),
        "[#[#2024-01-01 #2024-02-01 nil] #[#2024-02-01 #2024-03-15 nil] #[5 nil 1] #[1]]"
    );
    assert_eq!(eval(&format!("{ds} [(max ds) (min ds) (- ds #2024-01-01)]")), "[#2024-02-15 #2024-01-01 #[0 45 nil]]");
    assert_eq!(eval("(array [#2024-01-01 1])"), "error: array elements must all be numbers, all booleans, all strings, or all dates, got #2024-01-01 and 1");
}

#[test]
fn vectors_and_maps_destructure_in_let_define_and_parameters() {
    assert_eq!(eval("(let (([a b & rest] [1 2 3 4])) [a b rest])"), "[1 2 [3 4]]");
    assert_eq!(eval("(let (([a [b c] :as all] [1 [2 3]])) [a b c all])"), "[1 2 3 [1 [2 3]]]");
    assert_eq!(eval("(let (([a b c] [1])) [a b c])"), "[1 nil nil]");
    assert_eq!(eval("(let (({:keys [x y] :or {y 10} :as m} {:x 1})) [x y m])"), "[1 10 {:x 1}]");
    assert_eq!(
        eval("(let (({:name n :address {:street street}} {:name \"Ada\" :address {:street \"Main\"}})) [n street])"),
        "[\"Ada\" \"Main\"]"
    );
    assert_eq!(eval("(let* (([a b] [1 2]) (c (+ a b))) c)"), "3");
    assert_eq!(eval("(define [lo hi] [1 9]) (- hi lo)"), "8");
    assert_eq!(eval("(define {:keys [host port]} {:host \"h\" :port 80}) (str host \":\" port)"), "\"h:80\"");
    assert_eq!(eval("(define (dist [x1 y1] [x2 y2]) (+ (abs (- x2 x1)) (abs (- y2 y1)))) (dist [0 0] [3 4])"), "7");
    assert_eq!(eval("((lambda ({:keys [a]} & [first-rest]) [a first-rest]) {:a 1} 2 3)"), "[1 2]");
    assert_eq!(eval("(let loop (([x & more] [1 2 3]) (acc 0)) (if x (loop more (+ acc x)) acc))"), "6");
    assert_eq!(eval("(loop (([a b] [1 2]) (n 0)) (if (= n 3) [a b] (recur [b (+ a b)] (+ n 1))))"), "[5 8]");
    assert_eq!(eval("(let (([a b] '(1 2)) ([c] #[7 8]) ([d] \"hi\") ([e] nil)) [a b c d e])"), "[1 2 7 \"h\" nil]");
    assert_eq!(eval("(let (([_ b] [1 2])) b)"), "2");
    assert_eq!(eval("(define (f [a b]) (define [c d] [b a]) [c d]) (f [1 2])"), "[2 1]");
    assert_eq!(eval("(let (([a b] 5)) a)"), "error: cannot destructure 5 as a sequence");
    assert_eq!(eval("(let (({:keys [a]} 5)) a)"), "error: cannot destructure 5 as a map");
    assert!(eval("(let (([a 1] [1 2])) a)").contains("cannot bind to 1"));
}

#[test]
fn match_tests_and_destructures_values() {
    let shape = "(define (area s) (match s ([:circle r] (* 3 r r)) ([:rect w h] (* w h)) ({:keys [side]} (* side side)) (else 0)))";
    assert_eq!(eval(&format!("{shape} [(area [:circle 2]) (area [:rect 2 3]) (area {{:side 4}}) (area \"blob\")]")), "[12 6 16 0]");
    assert_eq!(
        eval("(match [1 2 3] ([a b] :when (> a b) \"desc\") ([a b] \"pair\") ([a & more] :when (> (count more) 1) (str \"long from \" a)) (_ \"other\"))"),
        "\"long from 1\""
    );
    assert_eq!(eval("(match 5 (0 \"zero\") (n :when (odd? n) \"odd\") (_ \"even\"))"), "\"odd\"");
    assert_eq!(eval("(match {:a 1 :b 2} ({:a x :b 2} (+ x 10)) (_ 0))"), "11");
    assert_eq!(eval("(match {:a 1} ({:a x :b 2} (+ x 10)) ({:keys [b] :or {b 5}} b))"), "5");
    assert_eq!(eval("(match '(1 (2 3)) ([x [y z]] (+ x y z)))"), "6");
    assert_eq!(eval("[(match \"hi\" (\"ho\" 1) (\"hi\" 2)) (match 'x ('x :symbol) (_ :other)) (match nil (nil :none) (_ :some)) (match #2024-01-01 (#2024-01-01 :new-year) (_ :day))]"), "[2 :symbol :none :new-year]");
    assert_eq!(eval("(match [1 2] ([a] a) ({:k v} v))"), "error: no match clause for [1 2]");
    assert_eq!(eval("(match [1 2] ([a b c] 1))"), "error: no match clause for [1 2]");
    assert!(eval("(match 1 (x))").contains("needs a body"));
    assert_eq!(eval("(define (first-two [a b & _]) (match [a b] ([1 x] x) ([x 1] (- x)) (_ 0))) [(first-two [1 5 9]) (first-two [7 1]) (first-two [2 2])]"), "[5 -7 0]");
}

#[test]
fn subtraction_accepts_an_array_first() {
    assert_eq!(eval("[(- #[3 4] 1) (- 10 #[3 4]) (- #[3 4] #[1 1])]"), "[#[2 3] #[7 6] #[2 3]]");
}

#[test]
fn padding_helpers() {
    assert_eq!(
        eval("[(string-pad-left \"7\" 3) (string-pad-left \"7\" 3 \"0\") (string-pad-right \"ab\" 4 \".\") (string-pad-left \"long\" 2) (string-pad-left #[\"7\" \"42\"] 3 \"0\")]"),
        "[\"  7\" \"007\" \"ab..\" \"long\" #[\"007\" \"042\"]]"
    );
    assert_eq!(eval("(string-pad-left \"x\" 3 \"ab\")"), "error: string-pad-left expects a single padding character, got \"ab\"");
}

#[test]
fn printing_goes_to_the_interpreter_output() {
    assert_eq!(eval_with_output("(println \"hi\" 1 [\"a\"])"), ("nil".to_string(), "hi 1 [a]\n".to_string()));
    assert_eq!(eval_with_output("(print \"a\") (print \"b\" 1)").1, "ab 1");
    assert_eq!(eval_with_output("(prn \"hi\" [\"a\"])").1, "\"hi\" [\"a\"]\n");
    assert_eq!(eval_with_output("(printf \"{} + {} = {:.1}\\n\" 1 2 3)"), ("nil".to_string(), "1 + 2 = 3.0\n".to_string()));
    assert_eq!(eval_with_output("(printf \"{:>3}\\n\" #[1 22])").1, "  1\n 22\n");
    assert_eq!(eval_with_output("(newline)").1, "\n");
    assert_eq!(eval_with_output("(println)").1, "\n");
    assert_eq!(eval_with_output("(for-each println [1 2])").1, "1\n2\n");
}

#[test]
fn prelude_function_combinators() {
    assert_eq!(eval("(identity 5)"), "5");
    assert_eq!(eval("((constantly 7) 1 2)"), "7");
    assert_eq!(eval("((complement even?) 3)"), "true");
    assert_eq!(eval("((partial + 1 2) 3 4)"), "10");
    assert_eq!(eval("[((comp inc inc) 1) ((comp) 4) ((comp str inc) 1)]"), "[3 4 \"2\"]");
    assert_eq!(eval("(map (comp inc (partial * 2)) [1 2])"), "[3 5]");
    assert_eq!(eval("comp"), "#<procedure comp>");
    assert_eq!(eval("((juxt inc dec) 5)"), "[6 4]");
}

#[test]
fn quasiquote_builds_data_with_holes() {
    assert_eq!(eval("`(1 ~(+ 1 1) 3)"), "(1 2 3)");
    assert_eq!(eval("`(a ~@[1 2] b)"), "(a 1 2 b)");
    assert_eq!(eval("`(~@[1 2] b)"), "(1 2 b)");
    assert_eq!(eval("`[1 ~(+ 1 1) ~@'(3 4)]"), "[1 2 3 4]");
    assert_eq!(eval("[`x `5 `\"s\" `:k `()]"), "[x 5 \"s\" :k ()]");
    assert_eq!(eval("`(1 (2 ~(+ 1 2)))"), "(1 (2 3))");
    assert_eq!(eval("`{:a ~(+ 1 1)}"), "{:a 2}");
    assert_eq!(eval("(let ((x 5) (list 0)) `(x ~x))"), "(x 5)");
    assert_eq!(eval("`(1 `(2 ~(3 ~(+ 1 3))))"), "(1 (quasiquote (2 (unquote (3 4)))))");
    assert_eq!(eval("(define-syntax m (syntax-rules () ((_ a) `(a ~a)))) (m (+ 1 2))"), "((+ 1 2) 3)");
    assert_eq!(eval("~x"), "error: unquote: not inside quasiquote");
    assert_eq!(eval("`(1 ~@5)"), "error: concat expects a collection, got 5");
}

#[test]
fn moved_locals_keep_value_semantics() {
    assert_eq!(eval("(define a [1]) (loop ((v a) (i 0)) (if (= i 2) [a v] (recur (conj v i) (+ i 1))))"), "[[1] [1 0 1]]");
    let captured_before_move = "(loop ((fs []) (v [1]) (i 0)) (if (= i 2) [(map (lambda (f) (f)) fs) v] (recur (conj fs (lambda () v)) (conj v i) (+ i 1))))";
    assert_eq!(eval(captured_before_move), "[[[1] [1 0]] [1 0 1]]");
    let captured_after_move = "(loop ((v [1]) (fs []) (i 0)) (if (= i 2) [(map (lambda (f) (f)) fs) v] (recur (conj v i) (conj fs (lambda () v)) (+ i 1))))";
    assert_eq!(eval(captured_after_move), "[[[1] [1 0]] [1 0 1]]");
    assert_eq!(eval("(define (g v) v) (define x [1]) [(g x) x (conj x 2) x]"), "[[1] [1] [1 2] [1]]");
    assert_eq!(eval("(define base [0]) [(reduce conj base [1 2]) base]"), "[[0 1 2] [0]]");
    assert_eq!(eval("(define m {:a 1}) (define (add m k) (assoc m k true)) [(add m :b) m]"), "[{:a 1, :b true} {:a 1}]");
    assert_eq!(eval("(define (f v n) (if (= n 0) v (f (conj v n) (- n 1)))) (define s #{}) [(f s 2) s]"), "[#{2 1} #{}]");
    assert_eq!(eval("(define (f v w) (recur-free v w)) (define (recur-free a b) [a b]) (define q [1]) (f q q)"), "[[1] [1]]");
    assert_eq!(eval("(define (f v) (let ((w v)) (conj w 1))) (define q [0]) [(f q) q]"), "[[0 1] [0]]");
}

#[test]
fn quote_wants_exactly_one_form() {
    assert!(eval("(quote)").starts_with("error:"));
    assert!(eval("(quote 1 2)").starts_with("error:"));
}
