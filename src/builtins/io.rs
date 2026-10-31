use super::native_vm;
use super::strings::join_display;
use crate::error::Error;
use crate::value::Value;
use crate::vm::{Arity, Globals, Vm};

pub fn install(globals: &mut Globals) {
    native_vm(globals, "print", Arity::AtLeast(0), |vm, args| emit(vm, &args, false, false));
    native_vm(globals, "println", Arity::AtLeast(0), |vm, args| emit(vm, &args, false, true));
    native_vm(globals, "prn", Arity::AtLeast(0), |vm, args| emit(vm, &args, true, true));
    native_vm(globals, "newline", Arity::Exact(0), |vm, args| emit(vm, &args, false, true));
}

fn io_error(e: std::io::Error) -> Error {
    Error::new("io-error", e.to_string())
}

fn emit(vm: &mut Vm, args: &[Value], readable: bool, newline: bool) -> Result<Value, Error> {
    let mut text = join_display(args, readable);
    if newline {
        text.push('\n');
    }
    let output = vm.output();
    output.write_all(text.as_bytes()).map_err(io_error)?;
    if !newline {
        output.flush().map_err(io_error)?;
    }
    Ok(Value::Nil)
}
