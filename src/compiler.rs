use std::collections::HashSet;

use crate::bytecode::{Capture, Op, Proto};
use crate::error::Error;
use crate::list::List;
use crate::symbol::Symbol;
use crate::value::Value;
use crate::vm::Globals;

pub struct Compiler<'a> {
    globals: &'a mut Globals,
    functions: Vec<FunctionState>,
}

struct FunctionState {
    proto: Proto,
    locals: Vec<Local>,
    captures: Vec<(Symbol, bool)>,
    depth: usize,
    boxed_names: HashSet<Symbol>,
}

struct Local {
    name: Symbol,
    slot: usize,
    boxed: bool,
}

#[derive(Clone, Copy)]
enum Resolved {
    Local(usize, bool),
    Upvalue(usize, bool),
    Global(usize),
}

impl FunctionState {
    fn new(name: Option<Symbol>) -> FunctionState {
        FunctionState {
            proto: Proto { name, ..Proto::default() },
            locals: Vec::new(),
            captures: Vec::new(),
            depth: 0,
            boxed_names: HashSet::new(),
        }
    }
}

fn syntax_error(form: &str, message: &str) -> Error {
    Error::syntax(format!("{form}: {message}"))
}

impl<'a> Compiler<'a> {
    pub fn new(globals: &'a mut Globals) -> Compiler<'a> {
        Compiler { globals, functions: Vec::new() }
    }

    pub fn compile_toplevel(mut self, form: &Value) -> Result<Proto, Error> {
        self.functions.push(FunctionState::new(None));
        self.compile(form, true)?;
        Ok(self.functions.pop().unwrap().proto)
    }

    fn current(&mut self) -> &mut FunctionState {
        self.functions.last_mut().expect("no function being compiled")
    }

    fn emit(&mut self, op: Op) {
        let f = self.current();
        f.depth = (f.depth as isize + op.stack_effect()) as usize;
        f.proto.code.push(op);
    }

    fn constant(&mut self, value: Value) -> u32 {
        let consts = &mut self.current().proto.consts;
        consts.push(value);
        (consts.len() - 1) as u32
    }

    fn emit_constant(&mut self, value: &Value) {
        match value {
            Value::Nil => self.emit(Op::Nil),
            Value::Bool(true) => self.emit(Op::True),
            Value::Bool(false) => self.emit(Op::False),
            other => {
                let index = self.constant(other.clone());
                self.emit(Op::Const(index));
            }
        }
    }

    fn finish_tail(&mut self, tail: bool) {
        if tail {
            self.emit(Op::Return);
        }
    }

    fn compile(&mut self, form: &Value, tail: bool) -> Result<(), Error> {
        match form {
            Value::Symbol(s) => self.compile_symbol(*s),
            Value::List(l) if !l.is_empty() => return self.compile_list(l, tail),
            Value::Vector(v) => {
                for item in v.iter() {
                    self.compile(item, false)?;
                }
                self.emit(Op::MakeVector(v.len() as u32));
            }
            Value::Map(m) => {
                for (k, v) in m.iter() {
                    self.compile(k, false)?;
                    self.compile(v, false)?;
                }
                self.emit(Op::MakeMap(m.len() as u32));
            }
            Value::Set(s) => {
                for item in s.iter() {
                    self.compile(item, false)?;
                }
                self.emit(Op::MakeSet(s.len() as u32));
            }
            other => self.emit_constant(other),
        }
        self.finish_tail(tail);
        Ok(())
    }

    fn compile_symbol(&mut self, name: Symbol) {
        match self.resolve(name) {
            Resolved::Local(slot, false) => self.emit(Op::GetLocal(slot as u32)),
            Resolved::Local(slot, true) => self.emit(Op::GetLocalCell(slot as u32)),
            Resolved::Upvalue(i, false) => self.emit(Op::GetUpvalue(i as u32)),
            Resolved::Upvalue(i, true) => self.emit(Op::GetUpvalueCell(i as u32)),
            Resolved::Global(slot) => self.emit(Op::GetGlobal(slot as u32)),
        }
    }

    fn resolve(&mut self, name: Symbol) -> Resolved {
        let top = self.functions.len() - 1;
        match self.resolve_in(top, name) {
            Some(r) => r,
            None => Resolved::Global(self.globals.slot(name)),
        }
    }

    fn resolve_in(&mut self, fi: usize, name: Symbol) -> Option<Resolved> {
        let f = &self.functions[fi];
        if let Some(local) = f.locals.iter().rev().find(|l| l.name == name) {
            return Some(Resolved::Local(local.slot, local.boxed));
        }
        if let Some(i) = f.captures.iter().position(|(n, _)| *n == name) {
            return Some(Resolved::Upvalue(i, f.captures[i].1));
        }
        if fi == 0 {
            return None;
        }
        let capture = match self.resolve_in(fi - 1, name)? {
            Resolved::Local(slot, boxed) => (Capture { from_local: true, index: slot as u32 }, boxed),
            Resolved::Upvalue(i, boxed) => (Capture { from_local: false, index: i as u32 }, boxed),
            Resolved::Global(_) => unreachable!(),
        };
        let f = &mut self.functions[fi];
        f.captures.push((name, capture.1));
        f.proto.captures.push(capture.0);
        Some(Resolved::Upvalue(f.captures.len() - 1, capture.1))
    }

    fn is_special(&mut self, head: &Value) -> Option<Symbol> {
        let Value::Symbol(s) = head else { return None };
        let top = self.functions.len() - 1;
        if self.resolve_in(top, *s).is_some() {
            return None;
        }
        Some(*s)
    }

    fn compile_list(&mut self, list: &List, tail: bool) -> Result<(), Error> {
        let head = list.first().unwrap();
        let args = list.rest();
        if let Some(sym) = self.is_special(head) {
            match &*sym.name() {
                "quote" => return self.compile_quote(&args, tail),
                _ => {}
            }
        }
        self.compile_call(list, tail)
    }

    fn compile_quote(&mut self, args: &List, tail: bool) -> Result<(), Error> {
        if args.len() != 1 {
            return Err(syntax_error("quote", "expected exactly one form"));
        }
        self.emit_constant(args.first().unwrap());
        self.finish_tail(tail);
        Ok(())
    }

    fn compile_call(&mut self, list: &List, tail: bool) -> Result<(), Error> {
        let mut argc = 0u32;
        for (i, item) in list.iter().enumerate() {
            self.compile(item, false)?;
            if i > 0 {
                argc += 1;
            }
        }
        self.emit(if tail { Op::TailCall(argc) } else { Op::Call(argc) });
        Ok(())
    }
}
