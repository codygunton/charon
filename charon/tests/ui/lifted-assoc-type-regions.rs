//@ revisions=poly,mono
//@ no-default-options
//@ charon-args=--preset=aeneas
//@[mono] charon-args=--monomorphize-local-only
//! `--lift-associated-types`: the lifted associated-type arguments of a type keep the regions of
//! the type's other arguments (rustc resolves the trait impls they come from with erased regions).
pub struct Edge<'a> {
    pub args: &'a [u32],
}

pub struct Summary<'a> {
    pub formals: &'a [u32],
    // `Peekable<IntoIter<Edge<'a>>, Edge<'a>>`
    pub dests: core::iter::Peekable<std::vec::IntoIter<Edge<'a>>>,
}

pub struct Words<'a> {
    // `Peekable<SplitWhitespace<'a>, &'a str>`
    pub words: core::iter::Peekable<core::str::SplitWhitespace<'a>>,
}

pub trait Array {
    type Item;
}
impl<T, const N: usize> Array for [T; N] {
    type Item = T;
}
pub struct Small<A: Array> {
    pub items: Vec<A::Item>,
}
pub struct Block<'a> {
    // `Small<[Edge<'a>; 2], Edge<'a>>`
    pub edges: Small<[Edge<'a>; 2]>,
}

pub fn mk<'a>(f: &'a [u32]) -> Summary<'a> {
    Summary {
        formals: f,
        dests: Vec::new().into_iter().peekable(),
    }
}

pub fn words<'a>(s: &'a str) -> Words<'a> {
    Words {
        words: s.split_whitespace().peekable(),
    }
}

pub fn first<'a, 'b>(b: &'b Block<'a>) -> Option<&'b Edge<'a>> {
    b.edges.items.first()
}
