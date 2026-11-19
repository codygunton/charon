//@ no-default-options
//@ charon-args=--preset=aeneas --monomorphize-local-only
//@ rustc-args=--edition=2021
//! Monomorphic instances whose type arguments contain closure types bind the regions of these
//! closures too: the closure's arguments are rebuilt from its own region structure (a captured
//! `&'a u32` and the `&'a u32` of its signature are the same region), so e.g.
//! `apply::<{closure}<'a>><'a>` and `Src::<Map<Iter<'a, u32>, {closure}<'b>>><'a, 'b>`.

pub struct Wrap<F> {
    pub f: F,
}

impl<F: Fn(u32) -> u32> Wrap<F> {
    pub fn call(&self, x: u32) -> u32 {
        (self.f)(x)
    }
}

pub fn apply<F: FnMut(&u32)>(v: &Vec<u32>, mut f: F) {
    for x in v.iter() {
        f(x)
    }
}

pub fn sum_with(v: &Vec<u32>) -> u32 {
    let mut acc = 0;
    apply(v, |x| acc += *x);
    acc
}

pub fn use_wrap(k: &u32) -> u32 {
    let w = Wrap { f: |x| x + *k };
    w.call(1)
}

pub struct Src<I> {
    pub it: I,
}

impl<I: Iterator<Item = u32>> Src<I> {
    pub fn next(&mut self) -> Option<u32> {
        self.it.next()
    }
}

pub fn first_plus(v: &[u32], k: &u32) -> Option<u32> {
    let mut s = Src {
        it: v.iter().map(move |x| x + k),
    };
    s.next()
}

pub fn main() {
    let v = vec![1, 2, 3];
    let _ = sum_with(&v);
    let _ = use_wrap(&4);
    let _ = first_plus(&v, &5);
}
