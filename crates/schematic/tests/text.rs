use schematic::Interpreter;

fn eval(src: &str) -> String {
    match Interpreter::new().eval_str(src) {
        Ok(v) => v.to_string(),
        Err(e) => format!("error: {e}"),
    }
}

#[test]
fn strings_split_into_characters() {
    assert_eq!(eval("(string-split \"héy\" \"\")"), "[\"h\" \"é\" \"y\"]");
    assert_eq!(eval("(string-split \"\" \"\")"), "[]");
    assert_eq!(eval("(string-split #[\"ab\" nil] \"\")"), "[[\"a\" \"b\"] nil]");
    assert_eq!(eval("(map char-code \"ab\")"), "[97 98]");
}

#[test]
fn character_classes_are_predicates_on_strings() {
    assert_eq!(eval("[(letter? \"a\") (letter? \"é\") (letter? \"a1\") (letter? \"\")]"), "[true true false false]");
    assert_eq!(eval("[(digit? \"42\") (digit? \"4a\") (whitespace? \" \\t\") (whitespace? \"a\")]"), "[true false true false]");
    assert_eq!(eval("[(upper-case? \"AB\") (upper-case? \"Ab\") (lower-case? \"ab\") (lower-case? \"aB\")]"), "[true false true false]");
    assert_eq!(eval("(letter? #[\"a\" \"1\" nil])"), "#[true false nil]");
    assert_eq!(eval("(string-join \"\" (filter letter? \"a,b c!\"))"), "\"abc\"");
    assert_eq!(eval("(letter? 5)"), "error: letter? expects a string, got 5");
}

#[test]
fn character_codes_round_trip() {
    assert_eq!(eval("[(char-code \"a\") (char-code \"é\") (code-char 97) (code-char 233)]"), "[97 233 \"a\" \"é\"]");
    assert_eq!(eval("(char-code \"ab\")"), "error: char-code expects a single character, got \"ab\"");
    assert_eq!(eval("(code-char -1)"), "error: code-char expects a character code, got -1");
    assert_eq!(eval("(code-char #[72 105])"), "#[\"H\" \"i\"]");
}

#[test]
fn raw_strings_keep_their_backslashes() {
    assert_eq!(eval("(string-length #\"\\d+\")"), "3");
    assert_eq!(eval("(= #\"a\\\"b\" \"a\\\\\\\"b\")"), "true");
    assert_eq!(eval("#\"\\\\\""), "\"\\\\\\\\\"");
    assert_eq!(eval("#\"open"), "error: 1:1: unterminated raw string");
}

#[test]
fn re_find_returns_the_first_match_or_its_groups() {
    assert_eq!(eval("(re-find \"order 66 and 99\" #\"\\d+\")"), "\"66\"");
    assert_eq!(eval("(re-find \"no digits\" #\"\\d+\")"), "nil");
    assert_eq!(eval("(re-find \"2024-03-15\" #\"(\\d+)-(\\d+)-(\\d+)\")"), "[\"2024-03-15\" \"2024\" \"03\" \"15\"]");
    assert_eq!(eval("(re-find \"ab\" #\"a(x)?b\")"), "[\"ab\" nil]");
    assert_eq!(eval("(re-find #[\"a1\" \"b\" nil] #\"\\d\")"), "#[\"1\" nil nil]");
    assert_eq!(eval("(re-find \"abc\" \"^b\")"), "nil");
    assert_eq!(eval("(re-find \"abc\" \"c$\")"), "\"c\"");
    assert_eq!(eval("(re-find \"A\" \"(?i)a\")"), "\"A\"");
    assert_eq!(eval("(re-find \"<a><b>\" #\"<.+?>\")"), "\"<a>\"");
    assert_eq!(eval("(re-find \"<a><b>\" #\"<.+>\")"), "\"<a><b>\"");
    assert_eq!(eval("(re-find \"aaaa\" \"a{2,3}\")"), "\"aaa\"");
    assert_eq!(eval("(re-find \"a\\nb\" \"a.b\")"), "nil");
    assert_eq!(eval("(re-find \"naïve x\" #\"\\w+\")"), "\"naïve\"");
    assert_eq!(eval("(re-find \"a.b\" #\"a\\.b\")"), "\"a.b\"");
    assert_eq!(eval("(re-find \"x_1-y\" #\"[a-z_][a-z0-9_]*\")"), "\"x_1\"");
}

#[test]
fn re_matches_needs_the_whole_string() {
    assert_eq!(eval("(re-matches \"2024\" #\"\\d+\")"), "\"2024\"");
    assert_eq!(eval("(re-matches \"2024x\" #\"\\d+\")"), "nil");
    assert_eq!(eval("(re-matches \"12345\" #\"\\d{5}\")"), "\"12345\"");
    assert_eq!(eval("(re-matches \"ab\" \"a|ab\")"), "\"ab\"");
    assert_eq!(eval("(re-matches \"ada@x.io\" #\"([^@]+)@(.+)\")"), "[\"ada@x.io\" \"ada\" \"x.io\"]");
}

#[test]
fn re_seq_and_re_contains_find_every_match() {
    assert_eq!(eval("(re-seq \"a1b22c333\" #\"\\d+\")"), "[\"1\" \"22\" \"333\"]");
    assert_eq!(eval("(re-seq \"k=v; a=b\" #\"(\\w+)=(\\w+)\")"), "[[\"k=v\" \"k\" \"v\"] [\"a=b\" \"a\" \"b\"]]");
    assert_eq!(eval("(re-seq \"cat dog cow\" #\"cat|cow\")"), "[\"cat\" \"cow\"]");
    assert_eq!(eval("(re-seq \"the cat scattered\" #\"\\bcat\\b\")"), "[\"cat\"]");
    assert_eq!(eval("(re-seq \"a,b;c\" #\"[^,;]+\")"), "[\"a\" \"b\" \"c\"]");
    assert_eq!(eval("(re-seq \"ab\" \"x*\")"), "[\"\" \"\" \"\"]");
    assert_eq!(eval("(re-seq \"none\" #\"\\d\")"), "[]");
    assert_eq!(eval("(re-contains? \"hello\" \"l+\")"), "true");
    assert_eq!(eval("(re-contains? #[\"ab\" \"cd\" nil] \"b\")"), "#[true false nil]");
}

#[test]
fn re_replace_uses_templates_or_functions() {
    assert_eq!(eval("(re-replace \"2024-03-15\" #\"(\\d+)-(\\d+)-(\\d+)\" \"$3/$2/$1\")"), "\"15/03/2024\"");
    assert_eq!(eval("(re-replace \"a1b2\" #\"\\d\" (lambda (m) (str \"<\" m \">\")))"), "\"a<1>b<2>\"");
    assert_eq!(eval("(re-replace \"a.b.c\" #\"\\.\" \"-\")"), "\"a-b-c\"");
    assert_eq!(eval("(re-replace \"5\" #\"\\d\" \"$$$0\")"), "\"$5\"");
    assert_eq!(eval("(re-replace \"aaa\" \"a*\" \"-\")"), "\"--\"");
    assert_eq!(eval("(re-replace #[\"a1\" nil] #\"\\d\" \"#\")"), "#[\"a#\" nil]");
    assert_eq!(eval("(re-replace \"k=v\" #\"(\\w)=(\\w)\" (lambda ([whole k v]) (str v \"=\" k)))"), "\"v=k\"");
    let slug = "(define (slugify s) (re-replace (re-replace (string-downcase s) #\"[^a-z0-9]+\" \"-\") #\"^-+|-+$\" \"\"))\n(slugify \"Hello, Wide World!\")";
    assert_eq!(eval(slug), "\"hello-wide-world\"");
}

#[test]
fn re_split_cuts_at_matches() {
    assert_eq!(eval("(re-split \"a1b22c\" #\"\\d+\")"), "[\"a\" \"b\" \"c\"]");
    assert_eq!(eval("(re-split \"a, b,c\" #\",\\s*\")"), "[\"a\" \"b\" \"c\"]");
    assert_eq!(eval("(re-split \"abc\" \"x*\")"), "[\"a\" \"b\" \"c\"]");
    assert_eq!(eval("(re-split \"\" \",\")"), "[\"\"]");
    assert_eq!(eval("(re-split #[\"a-b\" nil] \"-\")"), "[[\"a\" \"b\"] nil]");
}

#[test]
fn bad_patterns_are_errors() {
    assert_eq!(eval("(re-find \"a\" \"(\")"), "error: regex \"(\": missing )");
    assert_eq!(eval("(re-find \"a\" \"a{2,1}\")"), "error: regex \"a{2,1}\": repeat range 2,1 is out of order");
    assert_eq!(eval("(re-find \"a\" \"*\")"), "error: regex \"*\": nothing to repeat");
    assert_eq!(eval("(re-find \"a\" #\"\\1\")"), "error: regex \"\\\\1\": backreferences are not supported");
    assert_eq!(eval("(re-find 1 \"a\")"), "error: re-find expects a string, got 1");
}
