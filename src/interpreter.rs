use std::io::Write;
use std::rc::Rc;

use crate::builtins;
use crate::compiler::Compiler;
use crate::error::Error;
use crate::reader::read_all;
use crate::value::Value;
use crate::vm::Vm;

pub struct Interpreter {
    vm: Vm,
}

impl Default for Interpreter {
    fn default() -> Interpreter {
        Interpreter::new()
    }
}

impl Interpreter {
    pub fn new() -> Interpreter {
        let mut vm = Vm::new();
        builtins::install(&mut vm.globals);
        Interpreter { vm }
    }

    pub fn set_frame_limit(&mut self, limit: usize) {
        self.vm.set_frame_limit(limit);
    }

    pub fn set_output(&mut self, output: Box<dyn Write>) {
        self.vm.set_output(output);
    }

    pub fn eval_str(&mut self, src: &str) -> Result<Value, Error> {
        let mut result = Value::Nil;
        for form in read_all(src)? {
            result = self.eval(&form)?;
        }
        Ok(result)
    }

    pub fn eval(&mut self, form: &Value) -> Result<Value, Error> {
        let proto = Compiler::new(&mut self.vm.globals).compile_toplevel(form)?;
        self.vm.execute(Rc::new(proto))
    }
}
