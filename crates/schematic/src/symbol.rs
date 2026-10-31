use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct Symbol(u32);

#[derive(Default)]
struct Interner {
    names: Vec<Rc<str>>,
    originals: Vec<Option<Symbol>>,
    ids: HashMap<Rc<str>, u32>,
}

thread_local! {
    static INTERNER: RefCell<Interner> = RefCell::new(Interner::default());
}

impl Symbol {
    pub fn intern(name: &str) -> Symbol {
        INTERNER.with(|cell| {
            let mut interner = cell.borrow_mut();
            if let Some(&id) = interner.ids.get(name) {
                return Symbol(id);
            }
            let id = interner.names.len() as u32;
            let name: Rc<str> = Rc::from(name);
            interner.names.push(name.clone());
            interner.originals.push(None);
            interner.ids.insert(name, id);
            Symbol(id)
        })
    }

    pub fn fresh_alias(original: Symbol) -> Symbol {
        INTERNER.with(|cell| {
            let mut interner = cell.borrow_mut();
            let name = interner.names[original.0 as usize].clone();
            let id = interner.names.len() as u32;
            interner.names.push(name);
            interner.originals.push(Some(original));
            Symbol(id)
        })
    }

    pub fn original(self) -> Option<Symbol> {
        INTERNER.with(|cell| cell.borrow().originals[self.0 as usize])
    }

    pub fn root(self) -> Symbol {
        let mut symbol = self;
        while let Some(original) = symbol.original() {
            symbol = original;
        }
        symbol
    }

    pub fn id(self) -> u32 {
        self.0
    }

    pub fn name(self) -> Rc<str> {
        INTERNER.with(|cell| cell.borrow().names[self.0 as usize].clone())
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_name_interns_to_same_symbol() {
        assert_eq!(Symbol::intern("foo"), Symbol::intern("foo"));
    }

    #[test]
    fn different_names_are_different_symbols() {
        assert_ne!(Symbol::intern("foo"), Symbol::intern("bar"));
    }

    #[test]
    fn name_round_trips() {
        assert_eq!(&*Symbol::intern("lambda").name(), "lambda");
    }

    #[test]
    fn displays_as_its_name() {
        assert_eq!(Symbol::intern("x").to_string(), "x");
    }

    #[test]
    fn aliases_are_distinct_but_print_like_their_original() {
        let tmp = Symbol::intern("tmp");
        let alias = Symbol::fresh_alias(tmp);
        assert_ne!(alias, tmp);
        assert_ne!(alias, Symbol::fresh_alias(tmp));
        assert_eq!(alias.to_string(), "tmp");
        assert_eq!(alias.original(), Some(tmp));
        assert_eq!(tmp.original(), None);
        assert_eq!(Symbol::fresh_alias(alias).root(), tmp);
        assert_eq!(Symbol::intern("tmp"), tmp);
    }
}
