use std::rc::Rc;

use crate::value::Value;

const BITS: usize = 5;
const WIDTH: usize = 1 << BITS;
const MASK: usize = WIDTH - 1;

#[derive(Clone, Debug)]
pub struct Vector {
    len: usize,
    shift: usize,
    root: Rc<Node>,
    tail: Vec<Value>,
}

#[derive(Clone, Debug)]
enum Node {
    Branch(Vec<Rc<Node>>),
    Leaf(Vec<Value>),
}

impl Default for Vector {
    fn default() -> Vector {
        Vector::new()
    }
}

impl Vector {
    pub fn new() -> Vector {
        Vector { len: 0, shift: BITS, root: Rc::new(Node::Branch(Vec::new())), tail: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn tail_offset(&self) -> usize {
        self.len - self.tail.len()
    }

    fn chunk_at(&self, index: usize) -> (&[Value], usize) {
        let tail_offset = self.tail_offset();
        if index >= tail_offset {
            return (&self.tail, tail_offset);
        }
        let mut node = &*self.root;
        let mut level = self.shift;
        loop {
            match node {
                Node::Branch(children) => {
                    node = &children[(index >> level) & MASK];
                    level = level.saturating_sub(BITS);
                }
                Node::Leaf(values) => return (values, index & !MASK),
            }
        }
    }

    pub fn get(&self, index: usize) -> Option<&Value> {
        if index >= self.len {
            return None;
        }
        let (chunk, start) = self.chunk_at(index);
        Some(&chunk[index - start])
    }

    pub fn first(&self) -> Option<&Value> {
        self.get(0)
    }

    pub fn last(&self) -> Option<&Value> {
        self.tail.last()
    }

    pub fn push(&self, value: Value) -> Vector {
        let mut out = self.clone();
        out.push_mut(value);
        out
    }

    pub fn push_mut(&mut self, value: Value) {
        if self.tail.len() < WIDTH {
            self.tail.push(value);
            self.len += 1;
            return;
        }
        let leaf = Rc::new(Node::Leaf(std::mem::take(&mut self.tail)));
        if (self.len >> BITS) > (1 << self.shift) {
            let old_root = std::mem::replace(&mut self.root, Rc::new(Node::Branch(Vec::new())));
            let path = new_path(self.shift, leaf);
            self.root = Rc::new(Node::Branch(vec![old_root, path]));
            self.shift += BITS;
        } else {
            let last_index = self.len - 1;
            push_tail(Rc::make_mut(&mut self.root), self.shift, last_index, leaf);
        }
        self.tail.push(value);
        self.len += 1;
    }

    pub fn assoc(&self, index: usize, value: Value) -> Option<Vector> {
        if index == self.len {
            return Some(self.push(value));
        }
        if index > self.len {
            return None;
        }
        let mut out = self.clone();
        let tail_offset = out.tail_offset();
        if index >= tail_offset {
            out.tail[index - tail_offset] = value;
        } else {
            set_in(Rc::make_mut(&mut out.root), out.shift, index, value);
        }
        Some(out)
    }

    pub fn iter(&self) -> Iter<'_> {
        Iter { vec: self, index: 0, chunk: &[], chunk_start: 0 }
    }
}

fn new_path(level: usize, leaf: Rc<Node>) -> Rc<Node> {
    if level == 0 {
        leaf
    } else {
        Rc::new(Node::Branch(vec![new_path(level - BITS, leaf)]))
    }
}

fn push_tail(parent: &mut Node, level: usize, last_index: usize, leaf: Rc<Node>) {
    let Node::Branch(children) = parent else {
        unreachable!("leaf reached while pushing tail");
    };
    let subidx = (last_index >> level) & MASK;
    if level == BITS {
        children.push(leaf);
    } else if subidx < children.len() {
        push_tail(Rc::make_mut(&mut children[subidx]), level - BITS, last_index, leaf);
    } else {
        children.push(new_path(level - BITS, leaf));
    }
}

fn set_in(node: &mut Node, level: usize, index: usize, value: Value) {
    match node {
        Node::Leaf(values) => values[index & MASK] = value,
        Node::Branch(children) => {
            let child = Rc::make_mut(&mut children[(index >> level) & MASK]);
            set_in(child, level.saturating_sub(BITS), index, value);
        }
    }
}

pub struct Iter<'a> {
    vec: &'a Vector,
    index: usize,
    chunk: &'a [Value],
    chunk_start: usize,
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a Value;

    fn next(&mut self) -> Option<&'a Value> {
        if self.index >= self.vec.len {
            return None;
        }
        if self.index - self.chunk_start >= self.chunk.len() {
            let (chunk, start) = self.vec.chunk_at(self.index);
            self.chunk = chunk;
            self.chunk_start = start;
        }
        let item = &self.chunk[self.index - self.chunk_start];
        self.index += 1;
        Some(item)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.vec.len - self.index;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for Iter<'_> {}

impl FromIterator<Value> for Vector {
    fn from_iter<I: IntoIterator<Item = Value>>(iter: I) -> Vector {
        let mut out = Vector::new();
        for item in iter {
            out.push_mut(item);
        }
        out
    }
}

impl PartialEq for Vector {
    fn eq(&self, other: &Vector) -> bool {
        self.len == other.len && self.iter().eq(other.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ints(n: i64) -> Vector {
        (0..n).map(Value::Int).collect()
    }

    #[test]
    fn empty_vector() {
        let v = Vector::new();
        assert_eq!(v.len(), 0);
        assert!(v.is_empty());
        assert!(v.get(0).is_none());
        assert!(v.first().is_none());
        assert!(v.last().is_none());
    }

    #[test]
    fn push_and_get_small() {
        let v = Vector::new().push(Value::Int(1)).push(Value::Int(2));
        assert_eq!(v.len(), 2);
        assert_eq!(v.get(0), Some(&Value::Int(1)));
        assert_eq!(v.get(1), Some(&Value::Int(2)));
        assert_eq!(v.get(2), None);
        assert_eq!(v.first(), Some(&Value::Int(1)));
        assert_eq!(v.last(), Some(&Value::Int(2)));
    }

    #[test]
    fn get_across_many_levels() {
        for n in [33, 1057, 40_000] {
            let v = ints(n);
            assert_eq!(v.len(), n as usize);
            for i in 0..n {
                assert_eq!(v.get(i as usize), Some(&Value::Int(i)), "index {i} of {n}");
            }
        }
    }

    #[test]
    fn push_leaves_the_original_untouched() {
        let a = ints(1057);
        let b = a.push(Value::Int(-1));
        assert_eq!(a.len(), 1057);
        assert_eq!(b.len(), 1058);
        assert_eq!(a.get(1056), Some(&Value::Int(1056)));
        assert_eq!(b.get(1057), Some(&Value::Int(-1)));
    }

    #[test]
    fn assoc_replaces_one_slot_persistently() {
        let a = ints(1100);
        let b = a.assoc(5, Value::Int(-5)).unwrap();
        let c = b.assoc(1099, Value::Int(-1099)).unwrap();
        assert_eq!(a.get(5), Some(&Value::Int(5)));
        assert_eq!(b.get(5), Some(&Value::Int(-5)));
        assert_eq!(b.get(1099), Some(&Value::Int(1099)));
        assert_eq!(c.get(1099), Some(&Value::Int(-1099)));
        assert_eq!(c.get(5), Some(&Value::Int(-5)));
    }

    #[test]
    fn assoc_at_length_appends_and_beyond_is_none() {
        let v = ints(3).assoc(3, Value::Int(3)).unwrap();
        assert_eq!(v.len(), 4);
        assert!(v.assoc(10, Value::Nil).is_none());
    }

    #[test]
    fn iterates_in_order() {
        let items: Vec<i64> = ints(2000)
            .iter()
            .map(|v| match v {
                Value::Int(i) => *i,
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(items, (0..2000).collect::<Vec<_>>());
    }

    #[test]
    fn equality_is_structural() {
        assert_eq!(ints(100), ints(100));
        assert_ne!(ints(100), ints(101));
        assert_ne!(ints(2), ints(2).assoc(0, Value::Nil).unwrap());
    }

    #[test]
    fn prints_with_square_brackets() {
        let v = Value::vector([Value::Int(1), Value::string("a"), Value::keyword("k")]);
        assert_eq!(v.to_string(), "[1 \"a\" :k]");
        assert_eq!(Value::vector([]).to_string(), "[]");
    }

    #[test]
    fn vectors_and_lists_are_not_equal() {
        assert_ne!(Value::vector([Value::Int(1)]), Value::list([Value::Int(1)]));
    }
}
