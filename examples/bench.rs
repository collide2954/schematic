use std::time::Instant;

use schematic::{Interpreter, Value};

const CASES: &[(&str, &str, &str)] = &[
    (
        "tail-loop",
        "(define (count n) (if (= n 0) 'done (count (- n 1))))",
        "(count 1000000)",
    ),
    (
        "fib",
        "(define (fib n) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))",
        "(fib 25)",
    ),
    (
        "recur",
        "",
        "(loop ((i 0) (acc 0)) (if (= i 1000000) acc (recur (+ i 1) (+ acc i))))",
    ),
    (
        "closure",
        "(define (adder n) (lambda (x) (+ x n))) (define add3 (adder 3))",
        "(loop ((i 0) (acc 0)) (if (= i 300000) acc (recur (+ i 1) (add3 acc))))",
    ),
    (
        "vector",
        "",
        "(reduce + (loop ((i 0) (v [])) (if (= i 100000) v (recur (+ i 1) (conj v i)))))",
    ),
    (
        "map",
        "",
        "(count (map (lambda (x) (* x 2)) (range 100000)))",
    ),
    (
        "hash",
        "(define m (loop ((i 0) (m {})) (if (= i 50000) m (recur (+ i 1) (assoc m i (* i i))))))",
        "(loop ((i 0) (acc 0)) (if (= i 50000) acc (recur (+ i 1) (+ acc (get m i)))))",
    ),
    (
        "string",
        "",
        "(count (loop ((i 0) (s \"\")) (if (= i 2000) s (recur (+ i 1) (str s \"ab\")))))",
    ),
];

fn main() {
    println!("value size: {} bytes", std::mem::size_of::<Value>());
    let filter = std::env::args().nth(1);
    for (name, setup, body) in CASES {
        if filter.as_deref().is_some_and(|f| !name.contains(f)) {
            continue;
        }
        let mut interp = Interpreter::new();
        interp.eval_str(setup).expect("setup");
        interp.eval_str(body).expect("warmup");
        let mut best = f64::MAX;
        for _ in 0..12 {
            let start = Instant::now();
            interp.eval_str(body).expect("run");
            best = best.min(start.elapsed().as_secs_f64());
        }
        println!("{name:>10}: {:>9.2} ms", best * 1000.0);
    }
}
