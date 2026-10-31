use std::rc::Rc;

use crate::symbol::Symbol;
use crate::value::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Const(u32),
    Nil,
    True,
    False,
    Pop,
    PopUnder(u32),
    GetLocal(u32),
    SetLocal(u32),
    GetLocalCell(u32),
    SetLocalCell(u32),
    GetUpvalue(u32),
    GetUpvalueCell(u32),
    SetUpvalueCell(u32),
    GetGlobal(u32),
    SetGlobal(u32),
    DefineGlobal(u32),
    MakeCell,
    Jump(u32),
    JumpIfFalse(u32),
    JumpIfTrue(u32),
    JumpIfFalseOrPop(u32),
    JumpIfTrueOrPop(u32),
    Call(u32),
    TailCall(u32),
    Recur(u32),
    Return,
    Closure(u32),
    MakeVector(u32),
    MakeMap(u32),
    MakeSet(u32),
}

impl Op {
    pub fn stack_effect(self) -> isize {
        use Op::*;
        match self {
            Const(_) | Nil | True | False | GetLocal(_) | GetLocalCell(_) | GetUpvalue(_)
            | GetUpvalueCell(_) | GetGlobal(_) | Closure(_) => 1,
            Pop | JumpIfFalse(_) | JumpIfTrue(_) | JumpIfFalseOrPop(_) | JumpIfTrueOrPop(_) | Return => -1,
            PopUnder(n) | Call(n) | TailCall(n) | Recur(n) => -(n as isize),
            SetLocal(_) | SetLocalCell(_) | SetUpvalueCell(_) | SetGlobal(_) | DefineGlobal(_)
            | MakeCell | Jump(_) => 0,
            MakeVector(n) | MakeSet(n) => 1 - n as isize,
            MakeMap(n) => 1 - 2 * n as isize,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capture {
    pub from_local: bool,
    pub index: u32,
}

#[derive(Debug, Default)]
pub struct Proto {
    pub name: Option<Symbol>,
    pub params: usize,
    pub rest: bool,
    pub boxed_params: Vec<u32>,
    pub code: Vec<Op>,
    pub consts: Vec<Value>,
    pub protos: Vec<Rc<Proto>>,
    pub captures: Vec<Capture>,
}
