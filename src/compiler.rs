use std::collections::HashSet;
use std::rc::Rc;

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

#[derive(Default)]
struct Scan {
    set_targets: HashSet<Symbol>,
    defined: HashSet<Symbol>,
    nested_refs: HashSet<Symbol>,
}

fn scan_boxed<'a>(forms: impl Iterator<Item = &'a Value>) -> HashSet<Symbol> {
    let mut scan = Scan::default();
    for form in forms {
        scan.walk(form, 0);
    }
    let Scan { mut set_targets, defined, nested_refs } = scan;
    set_targets.extend(defined.intersection(&nested_refs));
    set_targets
}

impl Scan {
    fn walk(&mut self, form: &Value, depth: usize) {
        match form {
            Value::Symbol(s) => {
                if depth > 0 {
                    self.nested_refs.insert(*s);
                }
            }
            Value::List(l) => self.walk_list(l, depth),
            Value::Vector(v) => v.iter().for_each(|item| self.walk(item, depth)),
            Value::Set(s) => s.iter().for_each(|item| self.walk(item, depth)),
            Value::Map(m) => m.iter().for_each(|(k, v)| {
                self.walk(k, depth);
                self.walk(v, depth);
            }),
            _ => {}
        }
    }

    fn walk_all(&mut self, forms: &List, depth: usize) {
        forms.iter().for_each(|form| self.walk(form, depth));
    }

    fn walk_bindings(&mut self, bindings: &Value, depth: usize) {
        let Value::List(items) = bindings else { return };
        for binding in items.iter() {
            if let Value::List(pair) = binding {
                self.walk_all(&pair.rest(), depth);
            }
        }
    }

    fn define_name(&mut self, name: Symbol, depth: usize) {
        if depth == 0 {
            self.defined.insert(name);
        }
    }

    fn walk_list(&mut self, list: &List, depth: usize) {
        let Some(head) = list.first() else { return };
        let rest = list.rest();
        if let Value::Symbol(s) = head {
            match &*s.name() {
                "quote" => return,
                "set!" => {
                    if let Some(Value::Symbol(target)) = rest.first() {
                        self.set_targets.insert(*target);
                    }
                    self.walk_all(&rest.rest(), depth);
                    return;
                }
                "lambda" => {
                    self.walk_all(&rest.rest(), depth + 1);
                    return;
                }
                "define" => {
                    match rest.first() {
                        Some(Value::Symbol(name)) => {
                            self.define_name(*name, depth);
                            self.walk_all(&rest.rest(), depth);
                        }
                        Some(Value::List(signature)) => {
                            if let Some(Value::Symbol(name)) = signature.first() {
                                self.define_name(*name, depth);
                            }
                            self.walk_all(&rest.rest(), depth + 1);
                        }
                        _ => {}
                    }
                    return;
                }
                "let" | "let*" => {
                    if let Some(Value::Symbol(name)) = rest.first() {
                        self.define_name(*name, depth);
                        let after = rest.rest();
                        if let Some(bindings) = after.first() {
                            self.walk_bindings(bindings, depth);
                        }
                        self.walk_all(&after.rest(), depth + 1);
                        return;
                    }
                    if let Some(bindings) = rest.first() {
                        self.walk_bindings(bindings, depth);
                    }
                    self.walk_all(&rest.rest(), depth);
                    return;
                }
                _ => {}
            }
        }
        self.walk(head, depth);
        self.walk_all(&rest, depth);
    }
}

fn parse_bindings(form: &str, bindings: &Value) -> Result<Vec<(Symbol, Value)>, Error> {
    let Value::List(items) = bindings else {
        return Err(syntax_error(form, "expected a list of bindings"));
    };
    items
        .iter()
        .map(|binding| {
            if let Value::List(pair) = binding {
                let rest = pair.rest();
                if let (Some(Value::Symbol(name)), Some(init), 2) = (pair.first(), rest.first(), pair.len()) {
                    return Ok((*name, init.clone()));
                }
            }
            Err(syntax_error(form, &format!("bad binding {binding}, expected (name value)")))
        })
        .collect()
}

fn parse_params(params: &Value) -> Result<(Vec<Symbol>, bool), Error> {
    let items = match params {
        Value::Symbol(s) => return Ok((vec![*s], true)),
        Value::List(l) => l,
        other => return Err(syntax_error("lambda", &format!("expected parameter names, got {other}"))),
    };
    let mut names = Vec::new();
    let mut rest = false;
    let mut items = items.iter();
    while let Some(item) = items.next() {
        let Value::Symbol(name) = item else {
            return Err(syntax_error("lambda", &format!("{item} is not a valid parameter name")));
        };
        if &*name.name() == "&" {
            match (items.next(), items.next()) {
                (Some(Value::Symbol(rest_name)), None) => {
                    names.push(*rest_name);
                    rest = true;
                }
                _ => return Err(syntax_error("lambda", "expected exactly one name after &")),
            }
            break;
        }
        names.push(*name);
    }
    for (i, name) in names.iter().enumerate() {
        if names[..i].contains(name) {
            return Err(syntax_error("lambda", &format!("duplicate parameter {name}")));
        }
    }
    Ok((names, rest))
}

impl<'a> Compiler<'a> {
    pub fn new(globals: &'a mut Globals) -> Compiler<'a> {
        Compiler { globals, functions: Vec::new() }
    }

    pub fn compile_toplevel(mut self, form: &Value) -> Result<Proto, Error> {
        let mut f = FunctionState::new(None);
        f.boxed_names = scan_boxed(std::iter::once(form));
        self.functions.push(f);
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

    fn emit_jump(&mut self, op: Op) -> usize {
        self.emit(op);
        self.current().proto.code.len() - 1
    }

    fn patch_jump(&mut self, at: usize) {
        let target = self.current().proto.code.len() as u32;
        let code = &mut self.current().proto.code;
        code[at] = match code[at] {
            Op::Jump(_) => Op::Jump(target),
            Op::JumpIfFalse(_) => Op::JumpIfFalse(target),
            Op::JumpIfTrue(_) => Op::JumpIfTrue(target),
            Op::JumpIfFalseOrPop(_) => Op::JumpIfFalseOrPop(target),
            Op::JumpIfTrueOrPop(_) => Op::JumpIfTrueOrPop(target),
            other => unreachable!("patching a non-jump {other:?}"),
        };
    }

    fn depth(&mut self) -> usize {
        self.current().depth
    }

    fn set_depth(&mut self, depth: usize) {
        self.current().depth = depth;
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
                "if" => return self.compile_if(&args, tail),
                "define" => return self.compile_define(&args, tail),
                "set!" => return self.compile_set(&args, tail),
                "begin" => return self.compile_sequence(&args, tail),
                "let" => return self.compile_let(&args, tail, false),
                "let*" => return self.compile_let(&args, tail, true),
                "and" => return self.compile_and_or(&args, tail, true),
                "or" => return self.compile_and_or(&args, tail, false),
                "cond" => return self.compile_cond(&args, tail),
                "when" => return self.compile_when(&args, tail, true),
                "unless" => return self.compile_when(&args, tail, false),
                "lambda" => {
                    let Some(params) = args.first() else {
                        return Err(syntax_error("lambda", "expected a parameter list"));
                    };
                    self.compile_lambda(params, &args.rest(), None)?;
                    self.finish_tail(tail);
                    return Ok(());
                }
                _ => {}
            }
        }
        self.compile_call(list, tail)
    }

    fn compile_lambda(&mut self, params: &Value, body: &List, name: Option<Symbol>) -> Result<(), Error> {
        if body.is_empty() {
            return Err(syntax_error("lambda", "expected a body"));
        }
        let (names, rest) = parse_params(params)?;
        let mut f = FunctionState::new(name);
        f.proto.params = names.len() - rest as usize;
        f.proto.rest = rest;
        f.boxed_names = scan_boxed(body.iter());
        for (i, &name) in names.iter().enumerate() {
            let boxed = f.boxed_names.contains(&name);
            if boxed {
                f.proto.boxed_params.push(i as u32);
            }
            f.locals.push(Local { name, slot: i, boxed });
        }
        f.depth = names.len();
        self.functions.push(f);
        self.compile_body(body, true)?;
        let f = self.functions.pop().unwrap();
        let protos = &mut self.current().proto.protos;
        protos.push(Rc::new(f.proto));
        let index = (protos.len() - 1) as u32;
        self.emit(Op::Closure(index));
        Ok(())
    }

    fn compile_body(&mut self, body: &List, tail: bool) -> Result<(), Error> {
        let mut defined: Vec<Symbol> = Vec::new();
        for form in body.iter() {
            if let Some(name) = self.internal_define_name(form) {
                if !defined.contains(&name) {
                    defined.push(name);
                }
            }
        }
        if defined.is_empty() {
            return self.compile_sequence(body, tail);
        }
        let locals_before = self.current().locals.len();
        for &name in &defined {
            let slot = self.depth();
            self.emit(Op::Nil);
            let boxed = self.current().boxed_names.contains(&name);
            if boxed {
                self.emit(Op::MakeCell);
            }
            self.current().locals.push(Local { name, slot, boxed });
        }
        self.compile_sequence(body, tail)?;
        if !tail {
            self.emit(Op::PopUnder(defined.len() as u32));
        }
        self.current().locals.truncate(locals_before);
        Ok(())
    }

    fn internal_define_name(&mut self, form: &Value) -> Option<Symbol> {
        let Value::List(l) = form else { return None };
        if &*self.is_special(l.first()?)?.name() != "define" {
            return None;
        }
        let rest = l.rest();
        match rest.first()? {
            Value::Symbol(name) => Some(*name),
            Value::List(signature) => match signature.first()? {
                Value::Symbol(name) => Some(*name),
                _ => None,
            },
            _ => None,
        }
    }

    fn compile_named_let(&mut self, name: Symbol, args: &List, tail: bool) -> Result<(), Error> {
        let Some(bindings) = args.first() else {
            return Err(syntax_error("let", "expected bindings and a body"));
        };
        let body = args.rest();
        if body.is_empty() {
            return Err(syntax_error("let", "expected a body"));
        }
        let bindings = parse_bindings("let", bindings)?;
        let slot = self.depth();
        self.emit(Op::Nil);
        let boxed = self.current().boxed_names.contains(&name);
        if boxed {
            self.emit(Op::MakeCell);
        }
        self.current().locals.push(Local { name, slot, boxed });
        let params = Value::list(bindings.iter().map(|(n, _)| Value::Symbol(*n)));
        self.compile_lambda(&params, &body, Some(name))?;
        self.emit(if boxed { Op::SetLocalCell(slot as u32) } else { Op::SetLocal(slot as u32) });
        let local = self.current().locals.pop().unwrap();
        for (_, init) in &bindings {
            self.compile(init, false)?;
        }
        self.current().locals.push(local);
        let argc = bindings.len() as u32;
        self.emit(if tail { Op::TailCall(argc) } else { Op::Call(argc) });
        if !tail {
            self.emit(Op::PopUnder(1));
        }
        self.current().locals.pop();
        Ok(())
    }

    fn compile_let(&mut self, args: &List, tail: bool, sequential: bool) -> Result<(), Error> {
        let form = if sequential { "let*" } else { "let" };
        if let (Some(Value::Symbol(name)), false) = (args.first(), sequential) {
            return self.compile_named_let(*name, &args.rest(), tail);
        }
        let Some(bindings) = args.first() else {
            return Err(syntax_error(form, "expected bindings and a body"));
        };
        let body = args.rest();
        if body.is_empty() {
            return Err(syntax_error(form, "expected a body"));
        }
        let bindings = parse_bindings(form, bindings)?;
        let locals_before = self.current().locals.len();
        let mut pending = Vec::new();
        for (name, init) in &bindings {
            let slot = self.depth();
            self.compile(init, false)?;
            let boxed = self.current().boxed_names.contains(name);
            if boxed {
                self.emit(Op::MakeCell);
            }
            let local = Local { name: *name, slot, boxed };
            if sequential {
                self.current().locals.push(local);
            } else {
                pending.push(local);
            }
        }
        self.current().locals.extend(pending);
        self.compile_body(&body, tail)?;
        if !tail && !bindings.is_empty() {
            self.emit(Op::PopUnder(bindings.len() as u32));
        }
        self.current().locals.truncate(locals_before);
        Ok(())
    }

    fn is_lambda_form(&mut self, form: &Value) -> bool {
        let Value::List(l) = form else { return false };
        match l.first() {
            Some(head) => self.is_special(head).is_some_and(|s| &*s.name() == "lambda"),
            None => false,
        }
    }

    fn compile_and_or(&mut self, args: &List, tail: bool, is_and: bool) -> Result<(), Error> {
        if args.is_empty() {
            self.emit(if is_and { Op::True } else { Op::False });
            self.finish_tail(tail);
            return Ok(());
        }
        let depth = self.depth();
        let last = args.len() - 1;
        let mut jumps = Vec::new();
        for (i, form) in args.iter().enumerate() {
            if i == last {
                self.compile(form, tail)?;
            } else {
                self.compile(form, false)?;
                let op = if is_and { Op::JumpIfFalseOrPop(0) } else { Op::JumpIfTrueOrPop(0) };
                jumps.push(self.emit_jump(op));
            }
        }
        if jumps.is_empty() {
            return Ok(());
        }
        for jump in jumps {
            self.patch_jump(jump);
        }
        self.set_depth(depth + 1);
        self.finish_tail(tail);
        Ok(())
    }

    fn compile_cond(&mut self, args: &List, tail: bool) -> Result<(), Error> {
        let depth = self.depth();
        let clauses: Vec<&Value> = args.iter().collect();
        let mut end_jumps = Vec::new();
        let mut keep_jumps = Vec::new();
        let mut has_else = false;
        for (i, clause) in clauses.iter().enumerate() {
            let Value::List(parts) = clause else {
                return Err(syntax_error("cond", &format!("bad clause {clause}")));
            };
            let Some(test) = parts.first() else {
                return Err(syntax_error("cond", "empty clause"));
            };
            let body = parts.rest();
            let is_else = matches!(test, Value::Symbol(s) if &*s.name() == "else") && self.is_special(test).is_some();
            if is_else {
                if i != clauses.len() - 1 {
                    return Err(syntax_error("cond", "else must be the last clause"));
                }
                if body.is_empty() {
                    return Err(syntax_error("cond", "else clause needs a body"));
                }
                self.compile_sequence(&body, tail)?;
                has_else = true;
                break;
            }
            self.compile(test, false)?;
            if body.is_empty() {
                keep_jumps.push(self.emit_jump(Op::JumpIfTrueOrPop(0)));
                continue;
            }
            let next = self.emit_jump(Op::JumpIfFalse(0));
            self.compile_sequence(&body, tail)?;
            if !tail {
                end_jumps.push(self.emit_jump(Op::Jump(0)));
            }
            self.patch_jump(next);
            self.set_depth(depth);
        }
        if !has_else {
            self.emit(Op::Nil);
            self.finish_tail(tail);
        }
        if tail {
            if !keep_jumps.is_empty() {
                for jump in keep_jumps {
                    self.patch_jump(jump);
                }
                self.set_depth(depth + 1);
                self.emit(Op::Return);
            }
        } else {
            for jump in end_jumps.into_iter().chain(keep_jumps) {
                self.patch_jump(jump);
            }
            self.set_depth(depth + 1);
        }
        Ok(())
    }

    fn compile_when(&mut self, args: &List, tail: bool, when: bool) -> Result<(), Error> {
        let form = if when { "when" } else { "unless" };
        let Some(test) = args.first() else {
            return Err(syntax_error(form, "expected a condition"));
        };
        let body = args.rest();
        self.compile(test, false)?;
        let skip = self.emit_jump(if when { Op::JumpIfFalse(0) } else { Op::JumpIfTrue(0) });
        let depth = self.depth();
        self.compile_sequence(&body, tail)?;
        let to_end = if tail { None } else { Some(self.emit_jump(Op::Jump(0))) };
        self.patch_jump(skip);
        self.set_depth(depth);
        self.emit(Op::Nil);
        self.finish_tail(tail);
        if let Some(jump) = to_end {
            self.patch_jump(jump);
        }
        Ok(())
    }

    fn compile_sequence(&mut self, forms: &List, tail: bool) -> Result<(), Error> {
        if forms.is_empty() {
            self.emit(Op::Nil);
            self.finish_tail(tail);
            return Ok(());
        }
        let last = forms.len() - 1;
        for (i, form) in forms.iter().enumerate() {
            if i == last {
                self.compile(form, tail)?;
            } else {
                self.compile(form, false)?;
                self.emit(Op::Pop);
            }
        }
        Ok(())
    }

    fn compile_define(&mut self, args: &List, tail: bool) -> Result<(), Error> {
        let Some(target) = args.first() else {
            return Err(syntax_error("define", "expected a name"));
        };
        let body = args.rest();
        let name = match target {
            Value::Symbol(name) => {
                if body.len() > 1 {
                    return Err(syntax_error("define", "expected a single value"));
                }
                match body.first() {
                    Some(value) if self.is_lambda_form(value) => {
                        let Value::List(l) = value else { unreachable!() };
                        let lambda_args = l.rest();
                        self.compile_lambda(lambda_args.first().unwrap(), &lambda_args.rest(), Some(*name))?;
                    }
                    Some(value) => self.compile(value, false)?,
                    None => self.emit(Op::Nil),
                }
                *name
            }
            Value::List(signature) => {
                let Some(Value::Symbol(name)) = signature.first() else {
                    return Err(syntax_error("define", &format!("cannot define {target}")));
                };
                self.compile_lambda(&Value::List(signature.rest()), &body, Some(*name))?;
                *name
            }
            other => return Err(syntax_error("define", &format!("cannot define {other}"))),
        };
        let top = self.functions.len() - 1;
        let local = self.functions[top].locals.iter().rev().find(|l| l.name == name);
        match local.map(|l| (l.slot as u32, l.boxed)) {
            Some((slot, false)) => self.emit(Op::SetLocal(slot)),
            Some((slot, true)) => self.emit(Op::SetLocalCell(slot)),
            None if top == 0 => {
                let slot = self.globals.slot(name) as u32;
                self.emit(Op::DefineGlobal(slot));
                self.finish_tail(tail);
                return Ok(());
            }
            None => {
                return Err(syntax_error("define", "only allowed at the top level or at the start of a body"));
            }
        }
        self.emit(Op::Pop);
        self.emit_constant(&Value::Symbol(name));
        self.finish_tail(tail);
        Ok(())
    }

    fn compile_set(&mut self, args: &List, tail: bool) -> Result<(), Error> {
        let rest = args.rest();
        let (Some(Value::Symbol(name)), Some(value), 2) = (args.first(), rest.first(), args.len()) else {
            return Err(syntax_error("set!", "expected a variable name and a value"));
        };
        self.compile(value, false)?;
        match self.resolve(*name) {
            Resolved::Local(slot, false) => self.emit(Op::SetLocal(slot as u32)),
            Resolved::Local(slot, true) => self.emit(Op::SetLocalCell(slot as u32)),
            Resolved::Upvalue(i, true) => self.emit(Op::SetUpvalueCell(i as u32)),
            Resolved::Upvalue(_, false) => {
                return Err(syntax_error("set!", &format!("{name} cannot be assigned here")));
            }
            Resolved::Global(slot) => self.emit(Op::SetGlobal(slot as u32)),
        }
        self.finish_tail(tail);
        Ok(())
    }

    fn compile_if(&mut self, args: &List, tail: bool) -> Result<(), Error> {
        let count = args.len();
        if !(2..=3).contains(&count) {
            return Err(syntax_error("if", "expected a condition, a then-branch, and an optional else-branch"));
        }
        let mut parts = args.iter();
        let condition = parts.next().unwrap();
        let then_branch = parts.next().unwrap();
        let else_branch = parts.next();
        self.compile(condition, false)?;
        let to_else = self.emit_jump(Op::JumpIfFalse(0));
        let depth = self.depth();
        self.compile(then_branch, tail)?;
        let to_end = if tail { None } else { Some(self.emit_jump(Op::Jump(0))) };
        self.patch_jump(to_else);
        self.set_depth(depth);
        match else_branch {
            Some(form) => self.compile(form, tail)?,
            None => {
                self.emit(Op::Nil);
                self.finish_tail(tail);
            }
        }
        if let Some(to_end) = to_end {
            self.patch_jump(to_end);
        }
        Ok(())
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
