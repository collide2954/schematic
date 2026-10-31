use schematic::Interpreter;

fn eval(src: &str) -> String {
    match Interpreter::new().eval_str(src) {
        Ok(v) => v.to_string(),
        Err(e) => format!("error: {e}"),
    }
}

const MY_OR: &str = "(define-syntax my-or (syntax-rules () ((_) false) ((_ e) e) ((_ e r ...) (let ((t e)) (if t t (my-or r ...))))))";

#[test]
fn rules_match_by_shape_and_expand_recursively() {
    assert_eq!(eval(&format!("{MY_OR} [(my-or) (my-or nil 2) (my-or false nil) (my-or 1 2)]")), "[false 2 nil 1]");
    assert_eq!(eval("(define-syntax m (syntax-rules () ((_ a) a))) (define-syntax m2 (syntax-rules () ((_ a) (m (m a))))) (m2 3)"), "3");
    assert_eq!(eval("(define-syntax m (syntax-rules () ((_ a) a)))"), "m");
}

#[test]
fn template_bindings_do_not_capture_user_variables() {
    assert_eq!(eval(&format!("{MY_OR} (let ((t 5)) (my-or false t))")), "5");
    let swap = "(define-syntax swap! (syntax-rules () ((_ a b) (let ((tmp a)) (set! a b) (set! b tmp)))))";
    assert_eq!(eval(&format!("{swap} (let ((tmp 1) (other 2)) (swap! tmp other) [tmp other])")), "[2 1]");
}

#[test]
fn template_references_ignore_user_shadowing() {
    assert_eq!(eval("(define-syntax my-if (syntax-rules () ((_ c a b) (if c a b)))) (let ((if 0)) (my-if true 1 2))"), "1");
    assert_eq!(eval("(define-syntax make-pair (syntax-rules () ((_ a b) (list a b)))) (let ((list 0)) (make-pair 1 2))"), "(1 2)");
    assert_eq!(eval("(define-syntax twice (syntax-rules () ((_ e) (let ((v e)) (+ v v))))) (define (f v) (twice (* v 3))) (f 2)"), "12");
}

#[test]
fn literals_nested_ellipses_and_vector_patterns() {
    let my_cond = "(define-syntax my-cond (syntax-rules (else) ((_ (else e)) e) ((_ (c e) rest ...) (if c e (my-cond rest ...)))))";
    assert_eq!(eval(&format!("{my_cond} [(my-cond (false 1) (else 2)) (my-cond (true 1) (else 2))]")), "[2 1]");
    let my_let = "(define-syntax my-let (syntax-rules () ((_ ((n v) ...) body ...) ((lambda (n ...) body ...) v ...))))";
    assert_eq!(eval(&format!("{my_let} (my-let ((a 1) (b 2)) (+ a b))")), "3");
    assert_eq!(eval("(define-syntax flat (syntax-rules () ((_ (a ...) ...) '(a ... ...)))) (flat (1 2) (3))"), "(1 2 3)");
    assert_eq!(eval("(define-syntax vlet (syntax-rules () ((_ [n v] body ...) (let ((n v)) body ...)))) (vlet [x 3] (* x x))"), "9");
    assert_eq!(eval("(define-syntax last-of (syntax-rules () ((_ a ... z) z))) (last-of 1 2 3)"), "3");
    assert_eq!(eval("(define-syntax pairs (syntax-rules () ((_ (a b) ...) '((a ...) (b ...))))) (pairs (1 2) (3 4))"), "((1 3) (2 4))");
}

#[test]
fn quoted_template_symbols_are_plain_symbols() {
    assert_eq!(eval("(define-syntax q (syntax-rules () ((_) 'sym))) (= (q) 'sym)"), "true");
    assert_eq!(eval("(define-syntax q (syntax-rules () ((_ x) '(x y)))) (q 1)"), "(1 y)");
}

#[test]
fn macros_can_define_globals() {
    assert_eq!(eval("(define-syntax def2 (syntax-rules () ((_ n v) (define n v)))) (def2 z 9) z"), "9");
    assert_eq!(eval("(define-syntax defhelper (syntax-rules () ((_) (define helper 42)))) (defhelper) helper"), "42");
}

#[test]
fn macros_expanding_to_internal_defines_are_hoisted() {
    let src = "(define-syntax def2 (syntax-rules () ((_ n v) (define n v)))) (define (f) (def2 x 2) (* x 3)) (f)";
    assert_eq!(eval(src), "6");
    let spliced = "(define-syntax defs (syntax-rules () ((_ a b) (begin (define a 1) (define b 2))))) (define (f) (defs p q) (+ p q)) (f)";
    assert_eq!(eval(spliced), "3");
}

#[test]
fn macros_build_control_structures() {
    let while_macro = "(define-syntax while (syntax-rules () ((_ c body ...) (let loop () (when c body ... (loop))))))";
    assert_eq!(eval(&format!("{while_macro} (define i 0) (define acc []) (while (< i 3) (set! acc (conj acc i)) (set! i (+ i 1))) acc")), "[0 1 2]");
    let inc = "(define-syntax inc! (syntax-rules () ((_ v) (set! v (+ v 1))) ((_ v n) (set! v (+ v n)))))";
    assert_eq!(eval(&format!("{inc} (define x 0) (inc! x) (inc! x 10) x")), "11");
    let unless2 = "(define-syntax unless2 (syntax-rules () ((_ c b ...) (if c nil (begin b ...)))))";
    assert_eq!(eval(&format!("{unless2} (define (f n) (unless2 (= n 0) (f (- n 1)))) (f 3)")), "nil");
}

#[test]
fn macros_that_assign_captured_locals() {
    let inc = "(define-syntax inc! (syntax-rules () ((_ v) (set! v (+ v 1)))))";
    assert_eq!(eval(&format!("{inc} (define (f) (let ((n 0)) (define (bump) (inc! n)) (bump) (bump) n)) (f)")), "2");
    let my_let = "(define-syntax my-let (syntax-rules () ((_ ((n v) ...) body ...) ((lambda (n ...) body ...) v ...))))";
    assert_eq!(eval(&format!("{my_let} (define (f) (my-let ((n 0)) (define (bump) (set! n (+ n 1))) (bump) n)) (f)")), "1");
}

#[test]
fn a_local_binding_shadows_a_macro() {
    assert_eq!(eval("(define-syntax m (syntax-rules () ((_) 1))) (let ((m (lambda () 2))) (m))"), "2");
}

#[test]
fn macro_errors_are_reported() {
    assert_eq!(eval("(define-syntax m (syntax-rules () ((_ a) a))) (m 1 2)"), "error: no syntax-rules pattern matches (m 1 2)");
    assert_eq!(eval("(define-syntax m (syntax-rules () ((_ a ...) (a)))) (m 1 2)"), "error: pattern variable a is bound by ... and must be followed by ...");
    assert!(eval("(define-syntax m 5)").starts_with("error:"));
    assert!(eval("(define-syntax m (syntax-rules () (_ a)))").starts_with("error:"));
    assert!(eval("(define-syntax m (syntax-rules () ((_ a b ...) '(a ... b))))").starts_with("error:") || eval("(define-syntax m (syntax-rules () ((_ a b ...) '(a ... b)))) (m 1 2)").starts_with("error:"));
}
