use std::rc::Rc;

use crate::value::Value;
use crate::vm::{Arity, Globals, Native, NativeFn};

mod math;

pub fn install(globals: &mut Globals) {
    math::install(globals);
}

fn native(globals: &mut Globals, name: &'static str, arity: Arity, func: NativeFn) {
    globals.define(name, Value::Native(Rc::new(Native { name, arity, func })));
}
