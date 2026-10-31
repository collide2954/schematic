use std::rc::Rc;

use crate::value::Value;
use crate::vm::{Arity, Globals, Native, NativeFn};

mod collections;
mod core;
mod math;

pub fn install(globals: &mut Globals) {
    core::install(globals);
    math::install(globals);
    collections::install(globals);
}

fn native(globals: &mut Globals, name: &'static str, arity: Arity, func: NativeFn) {
    globals.define(name, Value::Native(Rc::new(Native { name, arity, func })));
}
