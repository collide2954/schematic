use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use crate::bytecode::{Op, Proto};
use crate::error::Error;
use crate::list::List;
use crate::symbol::Symbol;
use crate::value::Value;

#[derive(Debug)]
pub struct Closure {
    pub proto: Rc<Proto>,
    pub upvalues: Vec<Value>,
}

pub type NativeFn = fn(&[Value]) -> Result<Value, Error>;

#[derive(Clone, Copy, Debug)]
pub enum Arity {
    Exact(usize),
    AtLeast(usize),
    Between(usize, usize),
}

impl Arity {
    pub fn accepts(self, n: usize) -> bool {
        match self {
            Arity::Exact(k) => n == k,
            Arity::AtLeast(k) => n >= k,
            Arity::Between(lo, hi) => n >= lo && n <= hi,
        }
    }
}

impl fmt::Display for Arity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Arity::Exact(1) => write!(f, "1 argument"),
            Arity::Exact(k) => write!(f, "{k} arguments"),
            Arity::AtLeast(1) => write!(f, "at least 1 argument"),
            Arity::AtLeast(k) => write!(f, "at least {k} arguments"),
            Arity::Between(lo, hi) => write!(f, "{lo} to {hi} arguments"),
        }
    }
}

pub struct Native {
    pub name: &'static str,
    pub arity: Arity,
    pub func: NativeFn,
}

impl fmt::Debug for Native {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Native({})", self.name)
    }
}

#[derive(Default)]
pub struct Globals {
    slots: HashMap<Symbol, usize>,
    names: Vec<Symbol>,
    values: Vec<Option<Value>>,
}

impl Globals {
    pub fn slot(&mut self, name: Symbol) -> usize {
        if let Some(&slot) = self.slots.get(&name) {
            return slot;
        }
        let slot = self.values.len();
        self.slots.insert(name, slot);
        self.names.push(name);
        self.values.push(None);
        slot
    }

    pub fn get(&self, slot: usize) -> Option<&Value> {
        self.values[slot].as_ref()
    }

    pub fn set(&mut self, slot: usize, value: Value) {
        self.values[slot] = Some(value);
    }

    pub fn name(&self, slot: usize) -> Symbol {
        self.names[slot]
    }

    pub fn define(&mut self, name: &str, value: Value) {
        let slot = self.slot(Symbol::intern(name));
        self.set(slot, value);
    }

    pub fn lookup(&self, name: Symbol) -> Option<&Value> {
        self.slots.get(&name).and_then(|&slot| self.get(slot))
    }
}

struct Frame {
    closure: Rc<Closure>,
    ip: usize,
    base: usize,
}

const DEFAULT_FRAME_LIMIT: usize = 1_000_000;

pub struct Vm {
    pub globals: Globals,
    stack: Vec<Value>,
    frames: Vec<Frame>,
    frame_limit: usize,
}

impl Default for Vm {
    fn default() -> Vm {
        Vm::new()
    }
}

impl Vm {
    pub fn new() -> Vm {
        Vm {
            globals: Globals::default(),
            stack: Vec::new(),
            frames: Vec::new(),
            frame_limit: DEFAULT_FRAME_LIMIT,
        }
    }

    pub fn set_frame_limit(&mut self, limit: usize) {
        self.frame_limit = limit;
    }

    pub fn execute(&mut self, proto: Rc<Proto>) -> Result<Value, Error> {
        let closure = Rc::new(Closure { proto, upvalues: Vec::new() });
        let stack_base = self.stack.len();
        let frame_base = self.frames.len();
        self.stack.push(Value::Closure(closure.clone()));
        self.frames.push(Frame { closure, ip: 0, base: stack_base + 1 });
        let result = self.run(frame_base);
        if result.is_err() {
            self.frames.truncate(frame_base);
            self.stack.truncate(stack_base);
        }
        result
    }

    fn pop(&mut self) -> Value {
        self.stack.pop().expect("stack underflow")
    }

    fn top(&self) -> &Value {
        self.stack.last().expect("stack underflow")
    }

    fn cell(value: &Value) -> &Rc<RefCell<Value>> {
        match value {
            Value::Cell(c) => c,
            other => panic!("expected a cell, found {other}"),
        }
    }

    fn drain_top(&mut self, n: usize) -> std::vec::Drain<'_, Value> {
        let start = self.stack.len() - n;
        self.stack.drain(start..)
    }

    fn run(&mut self, stop_depth: usize) -> Result<Value, Error> {
        loop {
            let frame = self.frames.last_mut().expect("no frame");
            let op = frame.closure.proto.code[frame.ip];
            frame.ip += 1;
            let base = frame.base;
            match op {
                Op::Const(i) => {
                    let v = self.frames.last().unwrap().closure.proto.consts[i as usize].clone();
                    self.stack.push(v);
                }
                Op::Nil => self.stack.push(Value::Nil),
                Op::True => self.stack.push(Value::Bool(true)),
                Op::False => self.stack.push(Value::Bool(false)),
                Op::Pop => {
                    self.pop();
                }
                Op::PopUnder(n) => {
                    let top = self.pop();
                    let len = self.stack.len();
                    self.stack.truncate(len - n as usize);
                    self.stack.push(top);
                }
                Op::GetLocal(i) => {
                    let v = self.stack[base + i as usize].clone();
                    self.stack.push(v);
                }
                Op::SetLocal(i) => {
                    let v = self.top().clone();
                    self.stack[base + i as usize] = v;
                }
                Op::GetLocalCell(i) => {
                    let v = Vm::cell(&self.stack[base + i as usize]).borrow().clone();
                    self.stack.push(v);
                }
                Op::SetLocalCell(i) => {
                    let v = self.top().clone();
                    *Vm::cell(&self.stack[base + i as usize]).borrow_mut() = v;
                }
                Op::GetUpvalue(i) => {
                    let v = self.frames.last().unwrap().closure.upvalues[i as usize].clone();
                    self.stack.push(v);
                }
                Op::GetUpvalueCell(i) => {
                    let v = Vm::cell(&self.frames.last().unwrap().closure.upvalues[i as usize])
                        .borrow()
                        .clone();
                    self.stack.push(v);
                }
                Op::SetUpvalueCell(i) => {
                    let v = self.top().clone();
                    *Vm::cell(&self.frames.last().unwrap().closure.upvalues[i as usize])
                        .borrow_mut() = v;
                }
                Op::GetGlobal(i) => match self.globals.get(i as usize) {
                    Some(v) => {
                        let v = v.clone();
                        self.stack.push(v);
                    }
                    None => return Err(self.unbound(i as usize)),
                },
                Op::SetGlobal(i) => {
                    if self.globals.get(i as usize).is_none() {
                        return Err(self.unbound(i as usize));
                    }
                    let v = self.top().clone();
                    self.globals.set(i as usize, v);
                }
                Op::DefineGlobal(i) => {
                    let v = self.pop();
                    self.globals.set(i as usize, v);
                    self.stack.push(Value::Symbol(self.globals.name(i as usize)));
                }
                Op::MakeCell => {
                    let v = self.pop();
                    self.stack.push(Value::Cell(Rc::new(RefCell::new(v))));
                }
                Op::Jump(target) => self.frames.last_mut().unwrap().ip = target as usize,
                Op::JumpIfFalse(target) => {
                    if !self.pop().is_truthy() {
                        self.frames.last_mut().unwrap().ip = target as usize;
                    }
                }
                Op::JumpIfTrue(target) => {
                    if self.pop().is_truthy() {
                        self.frames.last_mut().unwrap().ip = target as usize;
                    }
                }
                Op::JumpIfFalseOrPop(target) => {
                    if self.top().is_truthy() {
                        self.pop();
                    } else {
                        self.frames.last_mut().unwrap().ip = target as usize;
                    }
                }
                Op::JumpIfTrueOrPop(target) => {
                    if self.top().is_truthy() {
                        self.frames.last_mut().unwrap().ip = target as usize;
                    } else {
                        self.pop();
                    }
                }
                Op::Call(argc) => self.call(argc as usize)?,
                Op::TailCall(argc) => {
                    if !self.tail_call(argc as usize)? {
                        if let Some(result) = self.do_return(stop_depth) {
                            return Ok(result);
                        }
                    }
                }
                Op::Return => {
                    if let Some(result) = self.do_return(stop_depth) {
                        return Ok(result);
                    }
                }
                Op::Closure(i) => {
                    let closure = self.make_closure(i as usize, base);
                    self.stack.push(Value::Closure(Rc::new(closure)));
                }
                Op::MakeVector(n) => {
                    let v = Value::vector(self.drain_top(n as usize));
                    self.stack.push(v);
                }
                Op::MakeSet(n) => {
                    let v = Value::set(self.drain_top(n as usize));
                    self.stack.push(v);
                }
                Op::MakeMap(n) => {
                    let items: Vec<Value> = self.drain_top(2 * n as usize).collect();
                    let mut items = items.into_iter();
                    let mut pairs = Vec::with_capacity(n as usize);
                    while let (Some(k), Some(v)) = (items.next(), items.next()) {
                        pairs.push((k, v));
                    }
                    self.stack.push(Value::map(pairs));
                }
            }
        }
    }

    fn unbound(&self, slot: usize) -> Error {
        Error::new("unbound", format!("{} is not defined", self.globals.name(slot)))
    }

    fn do_return(&mut self, stop_depth: usize) -> Option<Value> {
        let result = self.pop();
        let frame = self.frames.pop().expect("no frame");
        self.stack.truncate(frame.base - 1);
        if self.frames.len() == stop_depth {
            return Some(result);
        }
        self.stack.push(result);
        None
    }

    fn make_closure(&self, index: usize, base: usize) -> Closure {
        let parent = &self.frames.last().unwrap().closure;
        let proto = parent.proto.protos[index].clone();
        let upvalues = proto
            .captures
            .iter()
            .map(|c| {
                if c.from_local {
                    self.stack[base + c.index as usize].clone()
                } else {
                    parent.upvalues[c.index as usize].clone()
                }
            })
            .collect();
        Closure { proto, upvalues }
    }

    fn bind_args(&mut self, proto: &Proto, base: usize, argc: usize) -> Result<(), Error> {
        if proto.rest {
            if argc < proto.params {
                return Err(self.arity_error(proto, argc));
            }
            let rest: List = self.drain_top(argc - proto.params).collect();
            self.stack.push(Value::List(rest));
        } else if argc != proto.params {
            return Err(self.arity_error(proto, argc));
        }
        for &i in &proto.boxed_params {
            let slot = &mut self.stack[base + i as usize];
            let v = std::mem::replace(slot, Value::Nil);
            *slot = Value::Cell(Rc::new(RefCell::new(v)));
        }
        Ok(())
    }

    fn arity_error(&self, proto: &Proto, argc: usize) -> Error {
        let name = proto.name.map(|s| s.to_string()).unwrap_or_else(|| "procedure".to_string());
        let arity = if proto.rest { Arity::AtLeast(proto.params) } else { Arity::Exact(proto.params) };
        Error::new("arity-error", format!("{name} expects {arity}, got {argc}"))
    }

    fn call(&mut self, argc: usize) -> Result<(), Error> {
        let callee_index = self.stack.len() - argc - 1;
        let callee = self.stack[callee_index].clone();
        match callee {
            Value::Closure(closure) => {
                if self.frames.len() >= self.frame_limit {
                    return Err(Error::new("stack-overflow", "stack overflow"));
                }
                self.bind_args(&closure.proto, callee_index + 1, argc)?;
                self.frames.push(Frame { closure, ip: 0, base: callee_index + 1 });
                Ok(())
            }
            Value::Native(native) => {
                let result = self.call_native(&native, callee_index + 1, argc)?;
                self.stack.truncate(callee_index);
                self.stack.push(result);
                Ok(())
            }
            other => Err(Error::type_error(format!("{other} is not a function"))),
        }
    }

    fn call_native(&mut self, native: &Native, start: usize, argc: usize) -> Result<Value, Error> {
        if !native.arity.accepts(argc) {
            return Err(Error::new(
                "arity-error",
                format!("{} expects {}, got {argc}", native.name, native.arity),
            ));
        }
        (native.func)(&self.stack[start..start + argc])
    }

    fn tail_call(&mut self, argc: usize) -> Result<bool, Error> {
        let callee_index = self.stack.len() - argc - 1;
        let Value::Closure(closure) = self.stack[callee_index].clone() else {
            self.call(argc)?;
            return Ok(false);
        };
        let base = self.frames.last().unwrap().base;
        let dst = base - 1;
        for k in 0..=argc {
            self.stack.swap(dst + k, callee_index + k);
        }
        self.stack.truncate(dst + argc + 1);
        self.bind_args(&closure.proto, base, argc)?;
        let frame = self.frames.last_mut().unwrap();
        frame.closure = closure;
        frame.ip = 0;
        Ok(true)
    }
}
