use crate::map::Map;
use crate::value::Value;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Set(Map);

impl Set {
    pub fn new() -> Set {
        Set::default()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn contains(&self, value: &Value) -> bool {
        self.0.contains_key(value)
    }

    pub fn insert(&self, value: Value) -> Set {
        let mut out = self.clone();
        out.insert_mut(value);
        out
    }

    pub fn insert_mut(&mut self, value: Value) {
        self.0.insert_mut(value, Value::Nil);
    }

    pub fn remove(&self, value: &Value) -> Set {
        let mut out = self.clone();
        out.remove_mut(value);
        out
    }

    pub fn remove_mut(&mut self, value: &Value) -> bool {
        self.0.remove_mut(value)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Value> {
        self.0.keys()
    }
}

impl FromIterator<Value> for Set {
    fn from_iter<I: IntoIterator<Item = Value>>(iter: I) -> Set {
        let mut out = Set::new();
        for item in iter {
            out.insert_mut(item);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ints(items: &[i64]) -> Set {
        items.iter().map(|&i| Value::Int(i)).collect()
    }

    #[test]
    fn empty_set() {
        let s = Set::new();
        assert_eq!(s.len(), 0);
        assert!(s.is_empty());
        assert!(!s.contains(&Value::Int(1)));
    }

    #[test]
    fn insert_contains_and_ignores_duplicates() {
        let s = Set::new().insert(Value::Int(1)).insert(Value::Int(2)).insert(Value::Int(1));
        assert_eq!(s.len(), 2);
        assert!(s.contains(&Value::Int(1)));
        assert!(s.contains(&Value::Float(2.0)));
        assert!(!s.contains(&Value::Int(3)));
    }

    #[test]
    fn remove_is_persistent() {
        let a = ints(&[1, 2, 3]);
        let b = a.remove(&Value::Int(2));
        assert_eq!(a.len(), 3);
        assert_eq!(b.len(), 2);
        assert!(!b.contains(&Value::Int(2)));
        assert_eq!(b.remove(&Value::Int(99)), b);
    }

    #[test]
    fn holds_many_items() {
        let s: Set = (0..5000).map(Value::Int).collect();
        assert_eq!(s.len(), 5000);
        assert!((0..5000).all(|i| s.contains(&Value::Int(i))));
        assert_eq!(s.iter().count(), 5000);
    }

    #[test]
    fn equality_ignores_order() {
        assert_eq!(ints(&[1, 2, 3]), ints(&[3, 1, 2]));
        assert_ne!(ints(&[1, 2]), ints(&[1, 2, 3]));
    }

    #[test]
    fn small_sets_keep_insertion_order_when_printed() {
        let s = Value::set([Value::Int(3), Value::keyword("a"), Value::Int(1)]);
        assert_eq!(s.to_string(), "#{3 :a 1}");
        assert_eq!(Value::set([]).to_string(), "#{}");
    }

    #[test]
    fn sets_hash_and_compare_as_values() {
        let a = Value::set([Value::Int(1), Value::Int(2)]);
        let b = Value::set([Value::Int(2), Value::Int(1)]);
        assert_eq!(a, b);
        assert_eq!(a.hash_code(), b.hash_code());
        assert_ne!(a, Value::vector([Value::Int(1), Value::Int(2)]));
    }
}
