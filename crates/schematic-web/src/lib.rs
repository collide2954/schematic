use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

use schematic::reader::read_all;
use schematic::{Interpreter, Value};

type Buffer = Rc<RefCell<Vec<u8>>>;

thread_local! {
    static OUTPUT: Buffer = Rc::new(RefCell::new(Vec::new()));
    static INTERPRETER: RefCell<Interpreter> = RefCell::new(fresh());
    static RESULT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

struct Capture(Buffer);

impl Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn fresh() -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter.set_output(Box::new(Capture(OUTPUT.with(Rc::clone))));
    interpreter
}

pub fn run(source: &str) -> String {
    OUTPUT.with(|output| output.borrow_mut().clear());
    let mut text = String::new();
    let flush = |text: &mut String| {
        OUTPUT.with(|output| {
            text.push_str(&String::from_utf8_lossy(&output.borrow()));
            output.borrow_mut().clear();
        });
    };
    match read_all(source) {
        Err(e) => text.push_str(&format!("error: {e}\n")),
        Ok(forms) => {
            for form in &forms {
                let result = INTERPRETER.with(|interpreter| interpreter.borrow_mut().eval(form));
                flush(&mut text);
                match result {
                    Ok(Value::Nil) => {}
                    Ok(value) => text.push_str(&format!("{value}\n")),
                    Err(e) => {
                        text.push_str(&format!("error: {e}\n"));
                        break;
                    }
                }
            }
        }
    }
    text
}

#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut bytes = Vec::with_capacity(len);
    let pointer = bytes.as_mut_ptr();
    std::mem::forget(bytes);
    pointer
}

/// # Safety
/// `pointer` and `len` must come from `alloc`, which hands the bytes over to this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn eval(pointer: *mut u8, len: usize) -> usize {
    let source = unsafe { String::from_raw_parts(pointer, len, len) };
    let text = run(&source);
    RESULT.with(|result| {
        *result.borrow_mut() = text.into_bytes();
        result.borrow().len()
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn result_ptr() -> *const u8 {
    RESULT.with(|result| result.borrow().as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn reset() {
    INTERPRETER.with(|interpreter| *interpreter.borrow_mut() = fresh());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_forms_and_keeps_state_between_calls() {
        assert_eq!(run("(define x 20) (println \"hi\") (+ x 1)"), "x\nhi\n21\n");
        assert_eq!(run("(* x 2)"), "40\n");
        assert_eq!(run("(+ 1"), "error: 1:2: unclosed `(`\n");
        assert!(run("(error \"boom\") 5").starts_with("error: boom"));
        reset();
        assert!(run("x").starts_with("error:"));
    }
}
