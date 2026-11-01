use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::builtins;
use crate::compiler::Compiler;
use crate::error::Error;
use crate::reader::read_all;
use crate::symbol::Symbol;
use crate::value::Value;
use crate::vm::{USER, Vm};

const PRELUDE: &str = include_str!("prelude.scm");

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
        let mut interp = Interpreter { vm };
        interp.eval_str(PRELUDE).expect("prelude failed to load");
        interp.vm.globals.set_current(Symbol::intern(USER));
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

    pub fn current_namespace(&self) -> String {
        self.vm.globals.current().to_string()
    }

    pub fn set_load_path(&mut self, dirs: Vec<PathBuf>) {
        self.vm.globals.load_path = dirs;
    }

    pub fn add_load_path(&mut self, dir: PathBuf) {
        self.vm.globals.load_path.insert(0, dir);
    }

    pub fn user_globals(&self) -> impl Iterator<Item = (String, &Value)> {
        let current = self.vm.globals.current();
        self.vm.globals.vars(current).filter_map(|(name, value)| value.map(|v| (name.to_string(), v)))
    }

    pub fn global_names(&self) -> Vec<String> {
        self.vm.globals.visible_names()
    }

    pub fn global(&self, name: &str) -> Option<&Value> {
        self.vm.globals.lookup(Symbol::intern(name))
    }

    pub fn load_file(&mut self, path: &Path) -> Result<Value, Error> {
        builtins::namespaces::load_file(&mut self.vm, path)
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
