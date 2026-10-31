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
fn quote_wants_exactly_one_form() {
    assert!(eval("(quote)").starts_with("error:"));
    assert!(eval("(quote 1 2)").starts_with("error:"));
}
