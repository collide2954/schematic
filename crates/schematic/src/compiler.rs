use std::collections::HashSet;
use std::rc::Rc;

use crate::builtins::namespaces;
use crate::bytecode::{Capture, Intrinsic, Op, Proto};
use crate::error::Error;
use crate::expander::Expander;
use crate::list::List;
use crate::macros::{Macro, strip_aliases};
use crate::patterns;
use crate::symbol::Symbol;
use crate::value::Value;
use crate::vm::{Arity, Globals};

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
    movable: HashSet<Symbol>,
    label_floor: usize,
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
            movable: HashSet::new(),
            label_floor: 0,
        }
    }

    fn fusable(&self, count: usize) -> bool {
        self.proto.code.len() >= count && self.proto.code.len() - count >= self.label_floor
    }

    fn pop_op(&mut self) -> Op {
        let op = self.proto.code.pop().unwrap();
        self.depth = (self.depth as isize - op.stack_effect()) as usize;
        op
    }
}

fn local_slot(op: Op) -> Option<u16> {
    match op {
        Op::GetLocal(slot) | Op::TakeLocal(slot) => u16::try_from(slot).ok(),
        _ => None,
    }
}

fn count_symbols(form: &Value, counts: &mut std::collections::HashMap<Symbol, usize>) {
    match form {
        Value::Symbol(s) => *counts.entry(*s).or_insert(0) += 1,
        Value::List(l) => {
            if matches!(l.first(), Some(Value::Symbol(s)) if &*s.root().name() == "quote") {
                return;
            }
            l.iter().for_each(|item| count_symbols(item, counts));
        }
        Value::Vector(v) => v.iter().for_each(|item| count_symbols(item, counts)),
        Value::Map(m) => m.iter().for_each(|(k, v)| {
            count_symbols(k, counts);
            count_symbols(v, counts);
        }),
        Value::Set(s) => s.iter().for_each(|item| count_symbols(item, counts)),
        _ => {}
    }
}

pub(crate) fn syntax_error(form: &str, message: &str) -> Error {
    Error::syntax(format!("{form}: {message}"))
}

fn global_ref(name: &str) -> Value {
    Value::Symbol(Symbol::fresh_alias(Symbol::intern(name)))
}

fn call(name: &str, args: impl IntoIterator<Item = Value>) -> Value {
    Value::list(std::iter::once(global_ref(name)).chain(args))
}

fn quoted(value: &Value) -> Value {
    call("quote", [value.clone()])
}

fn tagged<'a>(form: &'a Value, tag: &str) -> Option<&'a Value> {
    let Value::List(l) = form else { return None };
    match (l.first(), l.len()) {
        (Some(Value::Symbol(s)), 2) if &*s.root().name() == tag => l.iter().nth(1),
        _ => None,
    }
}

fn quasi(form: &Value, depth: usize) -> Result<Value, Error> {
    match form {
        Value::List(l) if !l.is_empty() => {
            if let Some(x) = tagged(form, "unquote") {
                return Ok(if depth == 1 {
                    x.clone()
                } else {
                    call("list", [quoted(&Value::symbol("unquote")), quasi(x, depth - 1)?])
                });
            }
            if let Some(x) = tagged(form, "quasiquote") {
                return Ok(call("list", [quoted(&Value::symbol("quasiquote")), quasi(x, depth + 1)?]));
            }
            if tagged(form, "unquote-splicing").is_some() && depth == 1 {
                return Err(syntax_error("unquote-splicing", "only allowed inside a list or vector"));
            }
            quasi_seq(l.iter(), depth)
        }
        Value::Vector(v) => Ok(call("vec", [quasi_seq(v.iter(), depth)?])),
        Value::Map(m) => {
            let mut args = Vec::new();
            for (k, v) in m.iter() {
                args.push(quasi(k, depth)?);
                args.push(quasi(v, depth)?);
            }
            Ok(call("hash-map", args))
        }
        Value::Set(s) => {
            let items = s.iter().map(|item| quasi(item, depth)).collect::<Result<Vec<_>, _>>()?;
            Ok(call("hash-set", items))
        }
        Value::Symbol(_) | Value::List(_) => Ok(quoted(form)),
        other => Ok(other.clone()),
    }
}

fn quasi_seq<'a>(items: impl Iterator<Item = &'a Value>, depth: usize) -> Result<Value, Error> {
    let mut segments: Vec<Value> = Vec::new();
    let mut run: Vec<Value> = Vec::new();
    let mut starts_with_splice = false;
    for item in items {
        if let Some(x) = tagged(item, "unquote-splicing") {
            if depth == 1 {
                if !run.is_empty() {
                    segments.push(call("list", run.drain(..)));
                }
                if segments.is_empty() {
                    starts_with_splice = true;
                }
                segments.push(x.clone());
            } else {
                run.push(call("list", [quoted(&Value::symbol("unquote-splicing")), quasi(x, depth - 1)?]));
            }
            continue;
        }
        run.push(quasi(item, depth)?);
    }
    if !run.is_empty() {
        segments.push(call("list", run));
    }
    Ok(match segments.len() {
        0 => quoted(&Value::list([])),
        1 if !starts_with_splice => segments.pop().unwrap(),
        _ => {
            if starts_with_splice {
                segments.insert(0, call("list", []));
            }
            call("concat", segments)
        }
    })
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
            if depth > 0 {
                self.nested_refs.insert(*s);
            }
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
                        _ => self.walk_all(&rest.rest(), depth),
                    }
                    return;
                }
                "loop" => {
                    if let Some(bindings) = rest.first() {
                        self.walk_bindings(bindings, depth);
                    }
                    self.walk_all(&rest.rest(), depth + 1);
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
        let form = Expander::new(self.globals).expand(form)?;
        let mut f = FunctionState::new(None);
        f.boxed_names = scan_boxed(std::iter::once(&form));
        self.functions.push(f);
        self.compile(&form, true)?;
        Ok(self.functions.pop().unwrap().proto)
    }

    fn current(&mut self) -> &mut FunctionState {
        self.functions.last_mut().expect("no function being compiled")
    }

    fn emit(&mut self, op: Op) {
        if let Op::Binary(kind) = op
            && let Some(fused) = self.fuse_binary(kind) {
                return self.emit(fused);
            }
        let f = self.current();
        f.depth = (f.depth as isize + op.stack_effect()) as usize;
        f.proto.code.push(op);
    }

    fn fuse_binary(&mut self, kind: Intrinsic) -> Option<Op> {
        let f = self.current();
        let code = &f.proto.code;
        let last = *code.last()?;
        let before = if f.fusable(2) { Some(code[code.len() - 2]) } else { None };
        if !f.fusable(1) {
            return None;
        }
        match (before.and_then(local_slot), last) {
            (Some(a), Op::SmallInt(k)) => {
                f.pop_op();
                f.pop_op();
                Some(Op::BinaryLocalConst(kind, a, k))
            }
            (Some(a), _) if local_slot(last).is_some() => {
                let b = local_slot(last).unwrap();
                f.pop_op();
                f.pop_op();
                Some(Op::BinaryLocals(kind, a, b))
            }
            (_, Op::SmallInt(k)) => {
                f.pop_op();
                Some(Op::BinaryConst(kind, k))
            }
            (_, _) if local_slot(last).is_some() => {
                let b = local_slot(last).unwrap();
                f.pop_op();
                Some(Op::BinaryLocal(kind, b))
            }
            _ => None,
        }
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
            Value::Int(i) if i32::try_from(*i).is_ok() => self.emit(Op::SmallInt(*i as i32)),
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

    fn emit_jump_if_false(&mut self) -> usize {
        let f = self.current();
        let fused = match f.proto.code.last() {
            Some(Op::Binary(kind)) if f.fusable(1) && !matches!(kind, Intrinsic::Add | Intrinsic::Sub | Intrinsic::Mul) => {
                Some(*kind)
            }
            _ => None,
        };
        match fused {
            Some(kind) => {
                self.current().pop_op();
                self.emit_jump(Op::JumpUnless(kind, 0))
            }
            None => self.emit_jump(Op::JumpIfFalse(0)),
        }
    }

    fn patch_jump(&mut self, at: usize) {
        let target = self.current().proto.code.len() as u32;
        self.current().label_floor = target as usize;
        let code = &mut self.current().proto.code;
        code[at] = match code[at] {
            Op::Jump(_) => Op::Jump(target),
            Op::JumpIfFalse(_) => Op::JumpIfFalse(target),
            Op::JumpUnless(kind, _) => Op::JumpUnless(kind, target),
            Op::JumpIfTrue(_) => Op::JumpIfTrue(target),
            Op::JumpIfFalseOrPop(_) => Op::JumpIfFalseOrPop(target),
            Op::JumpIfTrueOrPop(_) => Op::JumpIfTrueOrPop(target),
            Op::PushCatch(_) => Op::PushCatch(target),
            Op::PushFinally(_) => Op::PushFinally(target),
            Op::PushEscape(_) => Op::PushEscape(target),
            other => unreachable!("patching a non-jump {other:?}"),
        };
    }

    fn depth(&mut self) -> usize {
        self.current().depth
    }

    fn set_depth(&mut self, depth: usize) {
        self.current().depth = depth;
    }

    fn compile_moving_args(&mut self, args: &List) -> Result<(), Error> {
        let args: Vec<&Value> = args.iter().collect();
        let mut later: Vec<std::collections::HashMap<Symbol, usize>> = Vec::with_capacity(args.len());
        for arg in &args {
            let mut counts = std::collections::HashMap::new();
            count_symbols(arg, &mut counts);
            later.push(counts);
        }
        for (i, arg) in args.iter().enumerate() {
            let movable: HashSet<Symbol> = later[i]
                .iter()
                .filter(|(symbol, count)| **count == 1 && later[i + 1..].iter().all(|c| !c.contains_key(*symbol)))
                .map(|(symbol, _)| *symbol)
                .collect();
            self.current().movable = movable;
            self.compile(arg, false)?;
        }
        self.current().movable.clear();
        Ok(())
    }

    fn compile(&mut self, form: &Value, tail: bool) -> Result<(), Error> {
        match form {
            Value::Symbol(s) => self.compile_symbol(*s, tail)?,
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

    fn compile_symbol(&mut self, name: Symbol, take: bool) -> Result<(), Error> {
        let take = take || self.current().movable.contains(&name);
        match self.resolve(name)? {
            Resolved::Local(slot, false) if take => self.emit(Op::TakeLocal(slot as u32)),
            Resolved::Local(slot, false) => self.emit(Op::GetLocal(slot as u32)),
            Resolved::Local(slot, true) => self.emit(Op::GetLocalCell(slot as u32)),
            Resolved::Upvalue(i, false) => self.emit(Op::GetUpvalue(i as u32)),
            Resolved::Upvalue(i, true) => self.emit(Op::GetUpvalueCell(i as u32)),
            Resolved::Global(slot) => self.emit(Op::GetGlobal(slot as u32)),
        }
        Ok(())
    }

    fn resolve(&mut self, name: Symbol) -> Result<Resolved, Error> {
        let top = self.functions.len() - 1;
        match self.resolve_in(top, name) {
            Some(r) => Ok(r),
            None => Ok(Resolved::Global(self.globals.reference(name.root())?)),
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
        Some(s.root())
    }

    fn macro_at_head(&mut self, form: &Value) -> Option<Rc<Macro>> {
        let Value::List(l) = form else { return None };
        let sym = self.is_special(l.first()?)?;
        self.globals.macro_for(sym)
    }

    fn expand_head(&mut self, form: &Value) -> Result<Value, Error> {
        let mut form = form.clone();
        while let Some(m) = self.macro_at_head(&form) {
            form = m.expand(&form)?;
        }
        Ok(form)
    }

    fn compile_define_syntax(&mut self, args: &List, tail: bool) -> Result<(), Error> {
        let rest = args.rest();
        let (Some(Value::Symbol(name)), Some(spec), 2) = (args.first(), rest.first(), args.len()) else {
            return Err(syntax_error("define-syntax", "expected a name and a syntax-rules form"));
        };
        let name = name.root();
        self.globals.define_macro(name, Rc::new(Macro::parse(spec)?));
        self.emit_constant(&Value::Symbol(name));
        self.finish_tail(tail);
        Ok(())
    }

    fn compile_list(&mut self, list: &List, tail: bool) -> Result<(), Error> {
        let head = list.first().unwrap();
        let args = list.rest();
        if let Some(sym) = self.is_special(head) {
            if let Some(m) = self.globals.macro_for(sym) {
                let expanded = m.expand(&Value::List(list.clone()))?;
                return self.compile(&expanded, tail);
            }
            match &*sym.name() {
                "quote" => return self.compile_quote(&args, tail),
                "quasiquote" => {
                    if args.len() != 1 {
                        return Err(syntax_error("quasiquote", "expected exactly one form"));
                    }
                    let rewritten = quasi(args.first().unwrap(), 1)?;
                    return self.compile(&rewritten, tail);
                }
                "unquote" | "unquote-splicing" => {
                    return Err(syntax_error(&sym.name(), "not inside quasiquote"));
                }
                "define-syntax" => return self.compile_define_syntax(&args, tail),
                "ns" => {
                    let Some(name) = args.first() else {
                        return Err(syntax_error("ns", "expected a namespace name"));
                    };
                    if !matches!(name, Value::Symbol(_)) {
                        return Err(syntax_error("ns", &format!("cannot switch to {name}")));
                    }
                    let form = namespaces::enter_form(name, args.iter().skip(1).cloned().collect());
                    return self.compile(&form, tail);
                }
                "if" => return self.compile_if(&args, tail),
                "define" => return self.compile_define(&args, tail),
                "set!" => return self.compile_set(&args, tail),
                "begin" => return self.compile_sequence(&args, tail),
                "let" => {
                    let args = patterns::rewrite_bindings("let", &args)?.unwrap_or(args);
                    return self.compile_let(&args, tail, false);
                }
                "let*" => {
                    let args = patterns::rewrite_bindings("let*", &args)?.unwrap_or(args);
                    return self.compile_let(&args, tail, true);
                }
                "match" => {
                    let form = patterns::rewrite_match(&args)?;
                    return self.compile(&form, tail);
                }
                "and" => return self.compile_and_or(&args, tail, true),
                "or" => return self.compile_and_or(&args, tail, false),
                "cond" => return self.compile_cond(&args, tail),
                "when" => return self.compile_when(&args, tail, true),
                "unless" => return self.compile_when(&args, tail, false),
                "loop" => {
                    let args = patterns::rewrite_bindings("loop", &args)?.unwrap_or(args);
                    return self.compile_loop(&args, tail);
                }
                "recur" => return self.compile_recur(&args, tail),
                "try" => return self.compile_try(&args, tail),
                "call/ec" => return self.compile_call_ec(&args, tail),
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
        let body = &patterns::without_return_type(body)?;
        if body.is_empty() {
            return Err(syntax_error("lambda", "expected a body"));
        }
        if let Some((params, body)) = patterns::rewrite_params(params, body)? {
            return self.compile_lambda(&params, &body, name);
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

    fn expand_body_form(&mut self, form: &Value, out: &mut Vec<Value>) -> Result<(), Error> {
        let expanded = self.expand_head(form)?;
        if let Value::List(l) = &expanded
            && let Some(head) = l.first() {
                let special = self.is_special(head).map(|s| s.name());
                if special.as_deref() == Some("begin") {
                    for inner in l.rest().iter() {
                        self.expand_body_form(inner, out)?;
                    }
                    return Ok(());
                }
                let rest = l.rest();
                if special.as_deref() == Some("define") && rest.first().is_some_and(patterns::is_pattern) {
                    let rewritten = patterns::rewrite_define(rest.first().unwrap(), &rest.rest())?;
                    return self.expand_body_form(&rewritten, out);
                }
            }
        out.push(expanded);
        Ok(())
    }

    fn compile_body(&mut self, body: &List, tail: bool) -> Result<(), Error> {
        let mut forms = Vec::new();
        for form in body.iter() {
            self.expand_body_form(form, &mut forms)?;
        }
        let body: List = forms.into_iter().collect();
        let body = &body;
        let mut defined: Vec<Symbol> = Vec::new();
        for form in body.iter() {
            if let Some(name) = self.internal_define_name(form)
                && !defined.contains(&name) {
                    defined.push(name);
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

    fn compile_call_ec(&mut self, args: &List, tail: bool) -> Result<(), Error> {
        if args.len() != 1 {
            return Err(syntax_error("call/ec", "expected exactly one procedure"));
        }
        let depth = self.depth();
        self.compile(args.first().unwrap(), false)?;
        let push = self.emit_jump(Op::PushEscape(0));
        self.emit(Op::Call(1));
        self.emit(Op::PopHandler);
        self.patch_jump(push);
        self.set_depth(depth + 1);
        self.finish_tail(tail);
        Ok(())
    }

    fn compile_try(&mut self, args: &List, tail: bool) -> Result<(), Error> {
        let mut body = Vec::new();
        let mut catch: Option<(Symbol, List)> = None;
        let mut finally: Option<List> = None;
        for form in args.iter() {
            let parts = match form {
                Value::List(l) => Some(l),
                _ => None,
            };
            let clause = parts.and_then(|l| l.first()).and_then(|head| self.is_special(head)).map(|s| s.name());
            match clause.as_deref() {
                Some("catch") => {
                    if catch.is_some() || finally.is_some() {
                        return Err(syntax_error("try", "catch must appear once, before finally"));
                    }
                    let rest = parts.unwrap().rest();
                    let Some(Value::Symbol(name)) = rest.first() else {
                        return Err(syntax_error("catch", "expected a variable name"));
                    };
                    catch = Some((*name, rest.rest()));
                }
                Some("finally") => {
                    if finally.is_some() {
                        return Err(syntax_error("try", "only one finally clause is allowed"));
                    }
                    finally = Some(parts.unwrap().rest());
                }
                _ => {
                    if catch.is_some() || finally.is_some() {
                        return Err(syntax_error("try", "body forms must come before catch and finally"));
                    }
                    body.push(form.clone());
                }
            }
        }
        let body: List = body.into_iter().collect();
        let depth = self.depth();
        let push_finally = finally.as_ref().map(|_| self.emit_jump(Op::PushFinally(0)));
        match &catch {
            Some((name, handler)) => {
                let push_catch = self.emit_jump(Op::PushCatch(0));
                self.compile_sequence(&body, false)?;
                self.emit(Op::PopHandler);
                let to_end = self.emit_jump(Op::Jump(0));
                self.patch_jump(push_catch);
                self.set_depth(depth + 1);
                let boxed = self.current().boxed_names.contains(name);
                if boxed {
                    self.emit(Op::MakeCell);
                }
                self.current().locals.push(Local { name: *name, slot: depth, boxed });
                self.compile_sequence(handler, false)?;
                self.emit(Op::PopUnder(1));
                self.current().locals.pop();
                self.patch_jump(to_end);
            }
            None => self.compile_sequence(&body, false)?,
        }
        if let (Some(fin), Some(push_finally)) = (&finally, push_finally) {
            self.emit(Op::PopHandler);
            self.compile_sequence(fin, false)?;
            self.emit(Op::Pop);
            let to_end = self.emit_jump(Op::Jump(0));
            self.patch_jump(push_finally);
            self.set_depth(depth + 1);
            self.compile_sequence(fin, false)?;
            self.emit(Op::Pop);
            self.emit(Op::Rethrow);
            self.patch_jump(to_end);
            self.set_depth(depth + 1);
        }
        self.finish_tail(tail);
        Ok(())
    }

    fn compile_loop(&mut self, args: &List, tail: bool) -> Result<(), Error> {
        let Some(bindings) = args.first() else {
            return Err(syntax_error("loop", "expected bindings and a body"));
        };
        let body = args.rest();
        if body.is_empty() {
            return Err(syntax_error("loop", "expected a body"));
        }
        let bindings = parse_bindings("loop", bindings)?;
        let params = Value::list(bindings.iter().map(|(n, _)| Value::Symbol(*n)));
        self.compile_lambda(&params, &body, Some(Symbol::intern("loop")))?;
        for (_, init) in &bindings {
            self.compile(init, false)?;
        }
        let argc = bindings.len() as u32;
        self.emit(if tail { Op::TailCall(argc) } else { Op::Call(argc) });
        Ok(())
    }

    fn compile_recur(&mut self, args: &List, tail: bool) -> Result<(), Error> {
        if self.functions.len() < 2 {
            return Err(syntax_error("recur", "not inside a function or loop"));
        }
        if !tail {
            return Err(syntax_error("recur", "must be in tail position"));
        }
        let proto = &self.current().proto;
        let expected = proto.params + proto.rest as usize;
        if args.len() != expected {
            let message = format!("expected {}, got {}", Arity::Exact(expected), args.len());
            return Err(syntax_error("recur", &message));
        }
        self.compile_moving_args(args)?;
        self.emit(Op::Recur(expected as u32));
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
            let next = self.emit_jump_if_false();
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
        let skip = if when { self.emit_jump_if_false() } else { self.emit_jump(Op::JumpIfTrue(0)) };
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
        let body = if matches!(target, Value::Symbol(_)) { patterns::without_return_type(&body)? } else { body };
        let defined = match target {
            Value::Symbol(name) => Some(*name),
            Value::List(signature) => match signature.first() {
                Some(Value::Symbol(name)) => Some(*name),
                _ => None,
            },
            _ => None,
        };
        if let Some(name) = defined
            && self.functions.len() == 1 && !self.functions[0].locals.iter().any(|l| l.name == name) {
                self.globals.define_slot(name.root());
            }
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
            pattern if patterns::is_pattern(pattern) => {
                let form = patterns::rewrite_define(pattern, &body)?;
                return self.compile(&form, tail);
            }
            other => return Err(syntax_error("define", &format!("cannot define {other}"))),
        };
        let top = self.functions.len() - 1;
        let local = self.functions[top].locals.iter().rev().find(|l| l.name == name);
        match local.map(|l| (l.slot as u32, l.boxed)) {
            Some((slot, false)) => self.emit(Op::SetLocal(slot)),
            Some((slot, true)) => self.emit(Op::SetLocalCell(slot)),
            None if top == 0 => {
                let slot = self.globals.define_slot(name.root()) as u32;
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
        match self.resolve(*name)? {
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
        let to_else = self.emit_jump_if_false();
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
        self.emit_constant(&strip_aliases(args.first().unwrap()));
        self.finish_tail(tail);
        Ok(())
    }

    fn intrinsic_op(&mut self, head: &Value, argc: usize) -> Option<Op> {
        let Value::Symbol(s) = head else { return None };
        let top = self.functions.len() - 1;
        if self.resolve_in(top, *s).is_some() {
            return None;
        }
        let Some(Value::Native(native)) = self.globals.lookup(s.root()) else { return None };
        match (native.intrinsic?, argc) {
            (Intrinsic::Not, 1) => Some(Op::Not),
            (Intrinsic::Not, _) => None,
            (kind, 2) => Some(Op::Binary(kind)),
            _ => None,
        }
    }

    fn compile_call(&mut self, list: &List, tail: bool) -> Result<(), Error> {
        let argc = list.len() - 1;
        if let Some(op) = self.intrinsic_op(list.first().unwrap(), argc) {
            for arg in list.rest().iter() {
                self.compile(arg, false)?;
            }
            self.emit(op);
            self.finish_tail(tail);
            return Ok(());
        }
        self.compile(list.first().unwrap(), false)?;
        if tail {
            self.compile_moving_args(&list.rest())?;
        } else {
            for item in list.rest().iter() {
                self.compile(item, false)?;
            }
        }
        let argc = argc as u32;
        self.emit(if tail { Op::TailCall(argc) } else { Op::Call(argc) });
        Ok(())
    }
}
