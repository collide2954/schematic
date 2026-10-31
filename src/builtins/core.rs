use super::native;
use crate::error::Error;
use crate::value::Value;
use crate::vm::{Arity, Globals};

pub fn install(globals: &mut Globals) {
    native(globals, "error", Arity::Between(1, 2), error);
    native(globals, "raise", Arity::Exact(1), |args| Err(Error::raise(args[0].clone())));
}

fn error(args: &[Value]) -> Result<Value, Error> {
    let Value::Str(_) = &args[0] else {
        return Err(Error::type_error(format!("error expects a message string, got {}", args[0])));
    };
    let mut pairs = vec![
        (Value::keyword("kind"), Value::keyword("user")),
        (Value::keyword("message"), args[0].clone()),
    ];
    if let Some(data) = args.get(1) {
        pairs.push((Value::keyword("data"), data.clone()));
    }
    Err(Error::raise(Value::map(pairs)))
}
