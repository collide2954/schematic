use std::io::Write;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::builtins;
use crate::compiler::Compiler;
use crate::error::Error;
use crate::reader::read_all;
use crate::symbol::Symbol;
use crate::value::Value;
use crate::vm::Vm;

const PRELUDE: &str = include_str!("prelude.scm");

pub struct Interpreter {
    vm: Vm,
    builtin_count: usize,
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
        let mut interp = Interpreter { vm, builtin_count: 0 };
        interp.eval_str(PRELUDE).expect("prelude failed to load");
        interp.builtin_count = interp.vm.globals.len();
        interp
    }

    pub fn set_frame_limit(&mut self, limit: usize) {
        self.vm.set_frame_limit(limit);
    }

    pub fn set_output(&mut self, output: Box<dyn Write>) {
        self.vm.set_output(output);
    }

    pub fn interrupt_handle(&self) -> Arc<AtomicBool> {
        self.vm.interrupt_handle()
    }

    pub fn user_globals(&self) -> impl Iterator<Item = (String, &Value)> {
        self.vm
            .globals
            .iter()
            .skip(self.builtin_count)
            .filter_map(|(name, value)| value.map(|v| (name.to_string(), v)))
    }

    pub fn global_names(&self) -> Vec<String> {
        let defined = self.vm.globals.iter().filter(|(_, value)| value.is_some()).map(|(name, _)| name.to_string());
        let mut names: Vec<String> =
            defined.chain(self.vm.globals.macro_names().map(|name| name.to_string())).filter(|n| !n.starts_with(' ')).collect();
        names.sort();
        names.dedup();
        names
    }

    pub fn global(&self, name: &str) -> Option<&Value> {
        self.vm.globals.lookup(Symbol::intern(name))
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
