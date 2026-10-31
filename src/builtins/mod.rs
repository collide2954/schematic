use std::rc::Rc;

use crate::value::Value;
use crate::vm::{Arity, Globals, Native, NativeFn, PureFn, VmFn};

mod collections;
mod core;
mod functional;
mod math;

pub fn install(globals: &mut Globals) {
    core::install(globals);
    math::install(globals);
    collections::install(globals);
    functional::install(globals);
}

fn native(globals: &mut Globals, name: &'static str, arity: Arity, func: PureFn) {
    let func = NativeFn::Pure(func);
    globals.define(name, Value::Native(Rc::new(Native { name, arity, func })));
}

fn native_vm(globals: &mut Globals, name: &'static str, arity: Arity, func: VmFn) {
    let func = NativeFn::Vm(func);
    globals.define(name, Value::Native(Rc::new(Native { name, arity, func })));
}
