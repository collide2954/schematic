use std::rc::Rc;

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
        Interpreter { vm: Vm::new() }
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
