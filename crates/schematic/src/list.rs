use std::rc::Rc;

use crate::value::Value;

#[derive(Clone, Debug, Default)]
pub struct List(Option<Rc<Node>>);

#[derive(Debug)]
struct Node {
    head: Value,
    tail: List,
}

impl List {
    pub fn new() -> List {
        List(None)
    }

    pub fn cons(self, head: Value) -> List {
        List(Some(Rc::new(Node { head, tail: self })))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }

    pub fn len(&self) -> usize {
        self.iter().count()
    }

    pub fn first(&self) -> Option<&Value> {
        self.0.as_ref().map(|n| &n.head)
    }

    pub fn rest(&self) -> List {
        match &self.0 {
            Some(n) => n.tail.clone(),
            None => List::new(),
        }
    }

    pub fn key(&self) -> Option<usize> {
        self.0.as_ref().map(|n| Rc::as_ptr(n) as usize)
    }

    pub fn ptr_eq(&self, other: &List) -> bool {
        match (&self.0, &other.0) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
    }

    pub fn iter(&self) -> Iter<'_> {
        Iter { node: self.0.as_deref() }
    }
}

pub struct Iter<'a> {
    node: Option<&'a Node>,
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a Value;

    fn next(&mut self) -> Option<&'a Value> {
        let node = self.node?;
        self.node = node.tail.0.as_deref();
        Some(&node.head)
    }
}

impl FromIterator<Value> for List {
    fn from_iter<I: IntoIterator<Item = Value>>(iter: I) -> List {
        let items: Vec<Value> = iter.into_iter().collect();
        items.into_iter().rev().fold(List::new(), List::cons)
    }
}

impl PartialEq for List {
    fn eq(&self, other: &List) -> bool {
        let mut a = self.iter();
        let mut b = other.iter();
        loop {
            match (a.next(), b.next()) {
                (None, None) => return true,
                (Some(x), Some(y)) if x == y => continue,
                _ => return false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;

    fn ints(items: &[i64]) -> List {
        items.iter().map(|&i| Value::Int(i)).collect()
    }

    #[test]
    fn empty_list_has_no_first_and_zero_length() {
        let l = List::new();
        assert!(l.is_empty());
        assert_eq!(l.len(), 0);
        assert!(l.first().is_none());
        assert!(l.rest().is_empty());
    }

    #[test]
    fn cons_puts_an_item_in_front() {
        let l = ints(&[2, 3]).cons(Value::Int(1));
        assert_eq!(l.len(), 3);
        assert_eq!(l.first(), Some(&Value::Int(1)));
        assert_eq!(l.rest(), ints(&[2, 3]));
    }

    #[test]
    fn collecting_preserves_order() {
        let items: Vec<Value> = ints(&[1, 2, 3]).iter().cloned().collect();
        assert_eq!(items, vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
    }

    #[test]
    fn equality_is_structural() {
        assert_eq!(ints(&[1, 2]), ints(&[1, 2]));
        assert_ne!(ints(&[1, 2]), ints(&[1, 2, 3]));
        assert_ne!(ints(&[1]), List::new());
    }

    #[test]
    fn rest_shares_structure() {
        let tail = ints(&[2, 3]);
        let l = tail.clone().cons(Value::Int(1));
        assert!(l.rest().ptr_eq(&tail));
    }

    #[test]
    fn prints_as_parenthesised_items() {
        let l: List = vec![Value::Int(1), Value::string("two"), Value::keyword("three")]
            .into_iter()
            .collect();
        assert_eq!(Value::List(l).to_string(), "(1 \"two\" :three)");
        assert_eq!(Value::List(List::new()).to_string(), "()");
    }

    #[test]
    fn empty_list_is_truthy_and_not_nil() {
        let empty = Value::List(List::new());
        assert!(empty.is_truthy());
        assert_ne!(empty, Value::Nil);
    }
}
