use std::rc::Rc;

use crate::value::Value;

const BITS: u32 = 5;
const MASK: u64 = (1 << BITS) - 1;
const MAX_SHIFT: u32 = 60;
const SMALL_LIMIT: usize = 8;

#[derive(Clone, Debug)]
pub struct Map {
    len: usize,
    repr: Repr,
}

#[derive(Clone, Debug)]
enum Repr {
    Small(Vec<(u64, Value, Value)>),
    Hashed(Rc<Node>),
}

#[derive(Clone, Debug)]
enum Node {
    Bitmap { bitmap: u32, entries: Vec<Entry> },
    Collision { hash: u64, pairs: Vec<(Value, Value)> },
}

#[derive(Clone, Debug)]
enum Entry {
    Pair(u64, Value, Value),
    Child(Rc<Node>),
}

enum Shape {
    Empty,
    Single(u64, Value, Value),
    Many,
}

impl Default for Map {
    fn default() -> Map {
        Map { len: 0, repr: Repr::Small(Vec::new()) }
    }
}

impl Default for Node {
    fn default() -> Node {
        Node::Bitmap { bitmap: 0, entries: Vec::new() }
    }
}

fn bit_for(hash: u64, shift: u32) -> u32 {
    1 << ((hash >> shift) & MASK)
}

fn index_of(bitmap: u32, bit: u32) -> usize {
    (bitmap & (bit - 1)).count_ones() as usize
}

impl Map {
    pub fn new() -> Map {
        Map::default()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn get(&self, key: &Value) -> Option<&Value> {
        self.get_hashed(key.hash_code(), key)
    }

    pub fn contains_key(&self, key: &Value) -> bool {
        self.get(key).is_some()
    }

    pub fn insert(&self, key: Value, value: Value) -> Map {
        let mut out = self.clone();
        out.insert_mut(key, value);
        out
    }

    pub fn insert_mut(&mut self, key: Value, value: Value) {
        self.insert_hashed(key.hash_code(), key, value);
    }

    pub fn remove(&self, key: &Value) -> Map {
        let mut out = self.clone();
        out.remove_mut(key);
        out
    }

    pub fn remove_mut(&mut self, key: &Value) -> bool {
        self.remove_hashed(key.hash_code(), key)
    }

    pub fn iter(&self) -> Iter<'_> {
        match &self.repr {
            Repr::Small(entries) => Iter { small: entries.iter(), stack: Vec::new(), collision: None },
            Repr::Hashed(root) => {
                let Node::Bitmap { entries, .. } = &**root else {
                    unreachable!("root is always a bitmap node");
                };
                Iter { small: [].iter(), stack: vec![entries.iter()], collision: None }
            }
        }
    }

    pub fn keys(&self) -> impl Iterator<Item = &Value> {
        self.iter().map(|(k, _)| k)
    }

    pub fn values(&self) -> impl Iterator<Item = &Value> {
        self.iter().map(|(_, v)| v)
    }

    fn get_hashed(&self, hash: u64, key: &Value) -> Option<&Value> {
        match &self.repr {
            Repr::Small(entries) => {
                entries.iter().find(|(h, k, _)| *h == hash && k == key).map(|(_, _, v)| v)
            }
            Repr::Hashed(root) => get_in(root, hash, key),
        }
    }

    fn insert_hashed(&mut self, hash: u64, key: Value, value: Value) {
        match &mut self.repr {
            Repr::Small(entries) => {
                if let Some(entry) = entries.iter_mut().find(|(h, k, _)| *h == hash && *k == key) {
                    entry.2 = value;
                    return;
                }
                if entries.len() < SMALL_LIMIT {
                    entries.push((hash, key, value));
                } else {
                    let mut root = Node::default();
                    for (h, k, v) in entries.drain(..) {
                        insert_in(&mut root, 0, h, k, v);
                    }
                    insert_in(&mut root, 0, hash, key, value);
                    self.repr = Repr::Hashed(Rc::new(root));
                }
                self.len += 1;
            }
            Repr::Hashed(root) => {
                if insert_in(Rc::make_mut(root), 0, hash, key, value) {
                    self.len += 1;
                }
            }
        }
    }

    fn remove_hashed(&mut self, hash: u64, key: &Value) -> bool {
        match &mut self.repr {
            Repr::Small(entries) => {
                let Some(i) = entries.iter().position(|(h, k, _)| *h == hash && k == key) else {
                    return false;
                };
                entries.remove(i);
            }
            Repr::Hashed(root) => {
                if get_in(root, hash, key).is_none() {
                    return false;
                }
                remove_in(Rc::make_mut(root), 0, hash, key);
            }
        }
        self.len -= 1;
        true
    }
}

fn get_in<'a>(mut node: &'a Node, hash: u64, key: &Value) -> Option<&'a Value> {
    let mut shift = 0;
    loop {
        match node {
            Node::Bitmap { bitmap, entries } => {
                let bit = bit_for(hash, shift);
                if bitmap & bit == 0 {
                    return None;
                }
                match &entries[index_of(*bitmap, bit)] {
                    Entry::Pair(h, k, v) => {
                        return if *h == hash && k == key { Some(v) } else { None };
                    }
                    Entry::Child(child) => {
                        node = child;
                        shift += BITS;
                    }
                }
            }
            Node::Collision { pairs, .. } => {
                return pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v);
            }
        }
    }
}

fn insert_in(node: &mut Node, shift: u32, hash: u64, key: Value, value: Value) -> bool {
    match node {
        Node::Collision { pairs, .. } => {
            if let Some(pair) = pairs.iter_mut().find(|(k, _)| *k == key) {
                pair.1 = value;
                false
            } else {
                pairs.push((key, value));
                true
            }
        }
        Node::Bitmap { bitmap, entries } => {
            let bit = bit_for(hash, shift);
            let idx = index_of(*bitmap, bit);
            if *bitmap & bit == 0 {
                entries.insert(idx, Entry::Pair(hash, key, value));
                *bitmap |= bit;
                return true;
            }
            match &mut entries[idx] {
                Entry::Child(child) => {
                    return insert_in(Rc::make_mut(child), shift + BITS, hash, key, value);
                }
                Entry::Pair(h, k, v) => {
                    if *h == hash && *k == key {
                        *v = value;
                        return false;
                    }
                }
            }
            let placeholder = Entry::Pair(0, Value::Nil, Value::Nil);
            let Entry::Pair(h0, k0, v0) = std::mem::replace(&mut entries[idx], placeholder) else {
                unreachable!();
            };
            let child = merge_pairs(shift + BITS, h0, k0, v0, hash, key, value);
            entries[idx] = Entry::Child(Rc::new(child));
            true
        }
    }
}

fn merge_pairs(shift: u32, h1: u64, k1: Value, v1: Value, h2: u64, k2: Value, v2: Value) -> Node {
    if shift > MAX_SHIFT {
        return Node::Collision { hash: h1, pairs: vec![(k1, v1), (k2, v2)] };
    }
    let b1 = bit_for(h1, shift);
    let b2 = bit_for(h2, shift);
    if b1 == b2 {
        let child = merge_pairs(shift + BITS, h1, k1, v1, h2, k2, v2);
        return Node::Bitmap { bitmap: b1, entries: vec![Entry::Child(Rc::new(child))] };
    }
    let entries = if b1 < b2 {
        vec![Entry::Pair(h1, k1, v1), Entry::Pair(h2, k2, v2)]
    } else {
        vec![Entry::Pair(h2, k2, v2), Entry::Pair(h1, k1, v1)]
    };
    Node::Bitmap { bitmap: b1 | b2, entries }
}

fn remove_in(node: &mut Node, shift: u32, hash: u64, key: &Value) {
    match node {
        Node::Collision { pairs, .. } => pairs.retain(|(k, _)| k != key),
        Node::Bitmap { bitmap, entries } => {
            let bit = bit_for(hash, shift);
            let idx = index_of(*bitmap, bit);
            let shape = match &mut entries[idx] {
                Entry::Pair(..) => Shape::Empty,
                Entry::Child(child) => {
                    let child = Rc::make_mut(child);
                    remove_in(child, shift + BITS, hash, key);
                    child.shape()
                }
            };
            match shape {
                Shape::Empty => {
                    entries.remove(idx);
                    *bitmap &= !bit;
                }
                Shape::Single(h, k, v) => entries[idx] = Entry::Pair(h, k, v),
                Shape::Many => {}
            }
        }
    }
}

impl Node {
    fn shape(&mut self) -> Shape {
        match self {
            Node::Bitmap { entries, .. } => match entries.as_mut_slice() {
                [] => Shape::Empty,
                [Entry::Pair(h, k, v)] => Shape::Single(
                    *h,
                    std::mem::replace(k, Value::Nil),
                    std::mem::replace(v, Value::Nil),
                ),
                _ => Shape::Many,
            },
            Node::Collision { hash, pairs } => match pairs.len() {
                0 => Shape::Empty,
                1 => {
                    let (k, v) = pairs.pop().unwrap();
                    Shape::Single(*hash, k, v)
                }
                _ => Shape::Many,
            },
        }
    }
}

pub struct Iter<'a> {
    small: std::slice::Iter<'a, (u64, Value, Value)>,
    stack: Vec<std::slice::Iter<'a, Entry>>,
    collision: Option<std::slice::Iter<'a, (Value, Value)>>,
}

impl<'a> Iterator for Iter<'a> {
    type Item = (&'a Value, &'a Value);

    fn next(&mut self) -> Option<(&'a Value, &'a Value)> {
        if let Some((_, k, v)) = self.small.next() {
            return Some((k, v));
        }
        loop {
            if let Some(pairs) = &mut self.collision {
                if let Some((k, v)) = pairs.next() {
                    return Some((k, v));
                }
                self.collision = None;
            }
            let top = self.stack.last_mut()?;
            match top.next() {
                None => {
                    self.stack.pop();
                }
                Some(Entry::Pair(_, k, v)) => return Some((k, v)),
                Some(Entry::Child(child)) => match &**child {
                    Node::Bitmap { entries, .. } => self.stack.push(entries.iter()),
                    Node::Collision { pairs, .. } => self.collision = Some(pairs.iter()),
                },
            }
        }
    }
}

impl FromIterator<(Value, Value)> for Map {
    fn from_iter<I: IntoIterator<Item = (Value, Value)>>(iter: I) -> Map {
        let mut out = Map::new();
        for (k, v) in iter {
            out.insert_mut(k, v);
        }
        out
    }
}

impl PartialEq for Map {
    fn eq(&self, other: &Map) -> bool {
        self.len == other.len && self.iter().all(|(k, v)| other.get(k) == Some(v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kw(name: &str) -> Value {
        Value::keyword(name)
    }

    fn int_map(n: i64) -> Map {
        (0..n).map(|i| (Value::Int(i), Value::Int(i * 10))).collect()
    }

    #[test]
    fn empty_map() {
        let m = Map::new();
        assert_eq!(m.len(), 0);
        assert!(m.is_empty());
        assert!(m.get(&kw("a")).is_none());
    }

    #[test]
    fn insert_and_get_mixed_keys() {
        let m = Map::new()
            .insert(kw("name"), Value::string("Ada"))
            .insert(Value::string("s"), Value::Int(1))
            .insert(Value::Int(7), Value::Bool(true));
        assert_eq!(m.len(), 3);
        assert_eq!(m.get(&kw("name")), Some(&Value::string("Ada")));
        assert_eq!(m.get(&Value::string("s")), Some(&Value::Int(1)));
        assert_eq!(m.get(&Value::Int(7)), Some(&Value::Bool(true)));
        assert!(m.contains_key(&Value::Int(7)));
        assert!(!m.contains_key(&Value::Int(8)));
    }

    #[test]
    fn inserting_an_existing_key_replaces_its_value() {
        let m = Map::new().insert(kw("a"), Value::Int(1)).insert(kw("a"), Value::Int(2));
        assert_eq!(m.len(), 1);
        assert_eq!(m.get(&kw("a")), Some(&Value::Int(2)));
    }

    #[test]
    fn int_and_float_keys_unify() {
        let m = Map::new().insert(Value::Int(1), kw("one"));
        assert_eq!(m.get(&Value::Float(1.0)), Some(&kw("one")));
    }

    #[test]
    fn holds_many_keys() {
        let m = int_map(10_000);
        assert_eq!(m.len(), 10_000);
        for i in 0..10_000 {
            assert_eq!(m.get(&Value::Int(i)), Some(&Value::Int(i * 10)), "key {i}");
        }
        assert!(m.get(&Value::Int(10_000)).is_none());
    }

    #[test]
    fn insert_leaves_the_original_untouched() {
        let a = int_map(1000);
        let b = a.insert(Value::Int(5), Value::Nil);
        assert_eq!(a.get(&Value::Int(5)), Some(&Value::Int(50)));
        assert_eq!(b.get(&Value::Int(5)), Some(&Value::Nil));
    }

    #[test]
    fn remove_drops_a_key_persistently() {
        let a = int_map(1000);
        let b = a.remove(&Value::Int(500));
        assert_eq!(a.len(), 1000);
        assert_eq!(b.len(), 999);
        assert!(a.contains_key(&Value::Int(500)));
        assert!(!b.contains_key(&Value::Int(500)));
        assert_eq!(b.get(&Value::Int(501)), Some(&Value::Int(5010)));
    }

    #[test]
    fn removing_an_absent_key_changes_nothing() {
        let a = int_map(10);
        assert_eq!(a.remove(&kw("nope")), a);
    }

    #[test]
    fn removing_everything_leaves_an_empty_map() {
        let mut m = int_map(5000);
        for i in 0..5000 {
            m = m.remove(&Value::Int(i));
        }
        assert!(m.is_empty());
        assert_eq!(m, Map::new());
    }

    #[test]
    fn colliding_hashes_are_handled() {
        let mut m = Map::new();
        for i in 0..12 {
            m.insert_hashed(42, Value::Int(i), Value::Int(-i));
        }
        assert_eq!(m.len(), 12);
        assert!(matches!(m.repr, Repr::Hashed(_)));
        for i in 0..12 {
            assert_eq!(m.get_hashed(42, &Value::Int(i)), Some(&Value::Int(-i)));
        }
        assert!(m.get_hashed(42, &Value::Int(99)).is_none());
        for i in 0..12 {
            assert!(m.remove_hashed(42, &Value::Int(i)));
            assert!(m.get_hashed(42, &Value::Int(i)).is_none());
        }
        assert!(m.is_empty());
    }

    #[test]
    fn iterates_every_pair_once() {
        let m = int_map(3000);
        let mut seen: Vec<i64> = m
            .iter()
            .map(|(k, v)| match (k, v) {
                (Value::Int(k), Value::Int(v)) if *v == k * 10 => *k,
                _ => panic!("bad pair"),
            })
            .collect();
        seen.sort();
        assert_eq!(seen, (0..3000).collect::<Vec<_>>());
    }

    #[test]
    fn equality_ignores_insertion_order() {
        let a = Map::new().insert(kw("a"), Value::Int(1)).insert(kw("b"), Value::Int(2));
        let b = Map::new().insert(kw("b"), Value::Int(2)).insert(kw("a"), Value::Int(1));
        assert_eq!(a, b);
        assert_ne!(a, b.insert(kw("a"), Value::Int(9)));
        assert_ne!(a, b.remove(&kw("a")));
    }

    #[test]
    fn prints_with_braces() {
        assert_eq!(Value::map([(kw("a"), Value::Int(1))]).to_string(), "{:a 1}");
        assert_eq!(Value::map([]).to_string(), "{}");
        let two = Value::map([(kw("b"), Value::Int(2)), (kw("a"), Value::Int(1))]).to_string();
        assert_eq!(two, "{:b 2, :a 1}");
    }

    #[test]
    fn small_maps_keep_insertion_order() {
        let m = Map::new()
            .insert(kw("z"), Value::Int(1))
            .insert(kw("a"), Value::Int(2))
            .insert(kw("m"), Value::Int(3))
            .insert(kw("a"), Value::Int(4))
            .remove(&kw("z"))
            .insert(kw("z"), Value::Int(5));
        let keys: Vec<String> = m.keys().map(|k| k.to_string()).collect();
        assert_eq!(keys, vec![":a", ":m", ":z"]);
        assert_eq!(m.get(&kw("a")), Some(&Value::Int(4)));
    }

    #[test]
    fn small_and_large_maps_compare_equal_by_content() {
        let mut big = int_map(12);
        for i in 7..12 {
            big = big.remove(&Value::Int(i));
        }
        assert_eq!(big, int_map(7));
        assert_eq!(int_map(7), big);
        assert_eq!(Value::Map(Rc::new(big)).hash_code(), Value::Map(Rc::new(int_map(7))).hash_code());
    }

    #[test]
    fn growing_past_the_small_limit_keeps_every_entry() {
        let mut m = Map::new();
        for i in 0..20 {
            m = m.insert(Value::Int(i), Value::Int(i));
            assert_eq!(m.len(), (i + 1) as usize);
            for j in 0..=i {
                assert_eq!(m.get(&Value::Int(j)), Some(&Value::Int(j)));
            }
        }
    }

    #[test]
    fn maps_hash_and_compare_as_values() {
        let a = Value::map([(kw("x"), Value::Int(1)), (kw("y"), Value::Int(2))]);
        let b = Value::map([(kw("y"), Value::Int(2)), (kw("x"), Value::Int(1))]);
        assert_eq!(a, b);
        assert_eq!(a.hash_code(), b.hash_code());
        let outer = Map::new().insert(a.clone(), Value::Bool(true));
        assert_eq!(outer.get(&b), Some(&Value::Bool(true)));
    }
}
