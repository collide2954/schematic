use std::rc::Rc;

use crate::bytecode::Intrinsic;
use crate::value::Value;
use crate::vm::{Arity, Globals, Native, NativeFn, OwnedFn, PureFn, VmFn};

mod collections;
mod core;
mod functional;
mod io;
pub mod math;
mod strings;

pub fn install(globals: &mut Globals) {
    core::install(globals);
    math::install(globals);
    collections::install(globals);
    functional::install(globals);
    strings::install(globals);
    io::install(globals);
}

fn native(globals: &mut Globals, name: &'static str, arity: Arity, func: PureFn) {
    let func = NativeFn::Pure(func);
    globals.define(name, Value::Native(Rc::new(Native { name, arity, func, intrinsic: None })));
}

fn native_intrinsic(globals: &mut Globals, name: &'static str, arity: Arity, func: PureFn, intrinsic: Intrinsic) {
    let func = NativeFn::Pure(func);
    let intrinsic = Some(intrinsic);
    globals.define(name, Value::Native(Rc::new(Native { name, arity, func, intrinsic })));
}

fn native_owned(globals: &mut Globals, name: &'static str, arity: Arity, func: OwnedFn) {
    let func = NativeFn::Owned(func);
    globals.define(name, Value::Native(Rc::new(Native { name, arity, func, intrinsic: None })));
}

fn native_vm(globals: &mut Globals, name: &'static str, arity: Arity, func: VmFn) {
    let func = NativeFn::Vm(func);
    globals.define(name, Value::Native(Rc::new(Native { name, arity, func, intrinsic: None })));
}
