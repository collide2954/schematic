use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::collections::expects;
use super::native_vm;
use crate::compiler::Compiler;
use crate::error::Error;
use crate::reader::read_all;
use crate::symbol::Symbol;
use crate::value::Value;
use crate::vm::{Arity, Globals, Native, NativeFn, Vm};

pub fn install(globals: &mut Globals) {
    native_vm(globals, "require", Arity::AtLeast(1), |vm, args| {
        for spec in &args {
            require_spec(vm, spec)?;
        }
        Ok(Value::Nil)
    });
    native_vm(globals, "refer", Arity::Between(1, 3), refer);
    native_vm(globals, "alias", Arity::Exact(2), |vm, args| {
        vm.globals.alias(symbol_arg("alias", &args[0])?, symbol_arg("alias", &args[1])?)?;
        Ok(Value::Nil)
    });
    native_vm(globals, "load", Arity::Exact(1), |vm, args| {
        let Value::Str(path) = &args[0] else { return Err(expects("load", "a path", &args[0])) };
        let path = locate(vm, Path::new(path.as_str()));
        load_file(vm, &path)
    });
    native_vm(globals, "current-ns", Arity::Exact(0), |vm, _| Ok(Value::Symbol(vm.globals.current())));
}

fn symbol_arg(function: &str, value: &Value) -> Result<Symbol, Error> {
    match value {
        Value::Symbol(s) => Ok(*s),
        other => Err(expects(function, "a symbol", other)),
    }
}

fn quote(value: Value) -> Value {
    Value::list([Value::symbol("quote"), value])
}

pub fn enter_form(name: &Value, clauses: Vec<Value>) -> Value {
    let native = Native { name: "ns", arity: Arity::Exact(2), func: NativeFn::Vm(enter), intrinsic: None };
    Value::list([Value::Native(Rc::new(native)), quote(name.clone()), quote(Value::list(clauses))])
}

fn enter(vm: &mut Vm, args: Vec<Value>) -> Result<Value, Error> {
    let name = symbol_arg("ns", &args[0])?;
    vm.globals.set_current(name);
    let Value::List(clauses) = &args[1] else { return Ok(Value::Nil) };
    for clause in clauses.iter() {
        let Value::List(items) = clause else {
            return Err(Error::syntax(format!("ns: expected a clause like (:require ...), got {clause}")));
        };
        match items.first() {
            Some(Value::Keyword(k)) if &*k.name() == "require" => {
                for spec in items.iter().skip(1) {
                    require_spec(vm, spec)?;
                }
            }
            _ => return Err(Error::syntax(format!("ns: unknown clause {clause}"))),
        }
    }
    Ok(Value::Nil)
}

fn require_spec(vm: &mut Vm, spec: &Value) -> Result<(), Error> {
    let (name, options): (Symbol, Vec<Value>) = match spec {
        Value::Symbol(s) => (*s, Vec::new()),
        Value::Vector(v) => match v.iter().next() {
            Some(Value::Symbol(s)) => (*s, v.iter().skip(1).cloned().collect()),
            _ => return Err(expects("require", "a namespace name first", spec)),
        },
        other => return Err(expects("require", "a namespace name or a vector like [name :as alias]", other)),
    };
    ensure_loaded(vm, name)?;
    let mut options = options.into_iter();
    while let Some(key) = options.next() {
        let Some(value) = options.next() else {
            return Err(Error::syntax(format!("require: {key} needs a value")));
        };
        match (&key, &value) {
            (Value::Keyword(k), Value::Symbol(alias)) if &*k.name() == "as" => vm.globals.alias(*alias, name)?,
            (Value::Keyword(k), Value::Keyword(all)) if &*k.name() == "refer" && &*all.name() == "all" => vm.globals.refer(name, None)?,
            (Value::Keyword(k), Value::Vector(names)) if &*k.name() == "refer" => {
                let names: Vec<Symbol> = names.iter().map(|n| symbol_arg("require", n)).collect::<Result<_, _>>()?;
                vm.globals.refer(name, Some(&names))?;
            }
            _ => return Err(Error::syntax(format!("require: unknown option {key} {value}"))),
        }
    }
    Ok(())
}

fn refer(vm: &mut Vm, args: Vec<Value>) -> Result<Value, Error> {
    let target = symbol_arg("refer", &args[0])?;
    match args.get(1..) {
        None | Some([]) => vm.globals.refer(target, None)?,
        Some([Value::Keyword(k), Value::Vector(names)]) if &*k.name() == "only" => {
            let names: Vec<Symbol> = names.iter().map(|n| symbol_arg("refer", n)).collect::<Result<_, _>>()?;
            vm.globals.refer(target, Some(&names))?;
        }
        Some(other) => return Err(Error::syntax(format!("refer: expected :only and a vector of names, got {}", Value::list(other.to_vec())))),
    }
    Ok(Value::Nil)
}

fn locate(vm: &Vm, path: &Path) -> PathBuf {
    if path.is_file() || path.is_absolute() {
        return path.to_path_buf();
    }
    vm.globals.load_path.iter().map(|dir| dir.join(path)).find(|candidate| candidate.is_file()).unwrap_or_else(|| path.to_path_buf())
}

fn ensure_loaded(vm: &mut Vm, name: Symbol) -> Result<(), Error> {
    if vm.globals.loading.contains(&name) {
        return Err(Error::new("io-error", format!("circular require of {name}")));
    }
    if vm.globals.has_namespace(name) {
        return Ok(());
    }
    let relative = PathBuf::from(format!("{}.scm", name.name().replace('.', "/")));
    let found = vm.globals.load_path.iter().map(|dir| dir.join(&relative)).find(|candidate| candidate.is_file());
    let Some(path) = found else {
        let dirs: Vec<String> = vm.globals.load_path.iter().map(|d| d.display().to_string()).collect();
        let message = format!("cannot find namespace {name}: no {} in {}", relative.display(), dirs.join(", "));
        return Err(Error::new("io-error", message));
    };
    vm.globals.loading.push(name);
    let result = load_file(vm, &path);
    vm.globals.loading.pop();
    result?;
    if !vm.globals.has_namespace(name) {
        return Err(Error::new("io-error", format!("{} does not declare the namespace {name}", path.display())));
    }
    Ok(())
}

pub fn load_file(vm: &mut Vm, path: &Path) -> Result<Value, Error> {
    let source = std::fs::read_to_string(path).map_err(|e| Error::new("io-error", format!("cannot read {}: {e}", path.display())))?;
    let forms = read_all(&source)?;
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let previous = vm.globals.current();
    vm.globals.load_path.insert(0, dir);
    let mut result = Ok(Value::Nil);
    for form in &forms {
        result = Compiler::new(&mut vm.globals).compile_toplevel(form).and_then(|proto| vm.execute(Rc::new(proto)));
        if result.is_err() {
            break;
        }
    }
    vm.globals.load_path.remove(0);
    vm.globals.set_current(previous);
    result
}
