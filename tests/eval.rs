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
fn quote_wants_exactly_one_form() {
    assert!(eval("(quote)").starts_with("error:"));
    assert!(eval("(quote 1 2)").starts_with("error:"));
}
