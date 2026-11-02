use schematic::Interpreter;

fn eval(src: &str) -> String {
    match Interpreter::new().eval_str(src) {
        Ok(schematic::Value::Str(s)) => s.to_string(),
        Ok(v) => v.to_string(),
        Err(e) => format!("error: {e}"),
    }
}

#[test]
fn json_text_becomes_values_and_back() {
    assert_eq!(eval(r#"(parse-json "{\"a\": [1, 2.5, true, null], \"b\": {\"c\": \"d\\n\"}}")"#), "{:a [1 2.5 true nil], :b {:c \"d\\n\"}}");
    assert_eq!(eval(r#"(parse-json "{\"a\": 1}" {:keys :strings})"#), "{\"a\" 1}");
    assert_eq!(eval(r#"(parse-json "1e3")"#), "1000");
    assert_eq!(eval(r#"(parse-json "12345678901234567890")"#), "1.2345678901234567e19");
    assert_eq!(eval(r#"(to-json {:a [1 2.5 nil] :b "x" :c :k :d #2024-03-15 :e #{1}})"#), r#"{"a":[1,2.5,null],"b":"x","c":"k","d":"2024-03-15","e":[1]}"#);
    assert_eq!(eval("(to-json (table {:x [1 2] :y [\"a\" nil]}))"), r#"[{"x":1,"y":"a"},{"x":2,"y":null}]"#);
    assert_eq!(eval("(to-json [1 \"two\"] {:pretty true})"), "[\n  1,\n  \"two\"\n]");
    assert_eq!(eval("(to-json '(1 2))"), "[1,2]");
    assert_eq!(eval("(to-json #[1 nil 3])"), "[1,null,3]");
    assert_eq!(eval("(parse-json (to-json {:a {:b [1 2]}}))"), "{:a {:b [1 2]}}");
}

#[test]
fn json_errors_name_the_problem() {
    assert_eq!(eval("(to-json inc)"), "error: to-json cannot encode a function");
    assert_eq!(eval("(parse-json \"[1,\")"), "error: parse-json: unexpected end of JSON");
    assert_eq!(eval("(parse-json \"[1] x\")"), "error: parse-json: unexpected 'x' at byte 4");
    assert_eq!(eval("(parse-json 5)"), "error: parse-json expects a string, got 5");
    assert_eq!(eval("(parse-json \"1\" {:keys :symbols})"), "error: parse-json expects :keywords or :strings for :keys, got :symbols");
    assert_eq!(eval("(to-json 1 {:pretty 2})"), "error: to-json expects true or false for :pretty, got 2");
    assert_eq!(eval("(read-json \"no/such/file.json\")").starts_with("error: cannot read no/such/file.json"), true);
}

#[test]
fn json_files_round_trip() {
    let dir = std::env::temp_dir().join(format!("schematic-json-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("out.json").to_string_lossy().replace('\\', "/");
    let source = format!("(write-json {{:n 1 :s [\"a\"]}} \"{path}\" {{:pretty true}}) (read-json \"{path}\")");
    assert_eq!(eval(&source), "{:n 1, :s [\"a\"]}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\n  \"n\": 1,\n  \"s\": [\n    \"a\"\n  ]\n}");
    std::fs::remove_dir_all(&dir).unwrap();
}
