use super::collections::expects;
use super::{native, native_vm};
use crate::docs;
use crate::error::Error;
use crate::value::Value;
use crate::vm::{Arity, Globals, Vm};

pub fn install(globals: &mut Globals) {
    native(globals, "doc-string", Arity::Exact(1), |args| {
        Ok(docs::describe(&name_of("doc-string", &args[0])?).map_or(Value::Nil, |text| Value::string(&text)))
    });
    native(globals, "apropos", Arity::Exact(1), |args| {
        let query = name_of("apropos", &args[0])?.to_lowercase();
        let names = docs::entries().iter().filter(|e| e.name.to_lowercase().contains(&query)).map(|e| Value::string(&e.name));
        Ok(Value::vector(names))
    });
    native_vm(globals, "print-doc", Arity::Exact(1), print_doc);
}

fn name_of(function: &str, value: &Value) -> Result<String, Error> {
    match value {
        Value::Symbol(s) | Value::Keyword(s) => Ok(s.name().to_string()),
        Value::Str(s) => Ok(s.to_string()),
        other => Err(expects(function, "a name", other)),
    }
}

fn print_doc(vm: &mut Vm, args: Vec<Value>) -> Result<Value, Error> {
    let name = name_of("doc", &args[0])?;
    let text = docs::describe(&name).unwrap_or_else(|| format!("No documentation for {name}"));
    let output = vm.output();
    output.write_all(format!("{text}\n").as_bytes()).map_err(|e| Error::new("io-error", e.to_string()))?;
    output.flush().map_err(|e| Error::new("io-error", e.to_string()))?;
    Ok(Value::Nil)
}
