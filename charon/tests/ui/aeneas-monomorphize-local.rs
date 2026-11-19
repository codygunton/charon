//@ no-default-options
//@ charon-args=--preset=aeneas --monomorphize-local-only
//@ rustc-args=--edition=2021
//! `--monomorphize-local-only`: local items are monomorphized, standard library items (`Vec`,
//! `Option`, slices, iterators, `Clone`, `FnOnce`, ...) stay polymorphic and are called with
//! generic arguments and trait references. Local impls of std traits list their items.

#[derive(Clone)]
pub struct Nop {
    pub x: u32,
}

pub struct Wrapper<T> {
    pub items: Vec<T>,
}

impl<T: Clone> Wrapper<T> {
    pub fn push_clone(&mut self, x: &T) {
        self.items.push(x.clone());
    }

    pub fn first_mapped(&self) -> Option<T> {
        self.items.first().map(|x| x.clone())
    }

    pub fn duplicate(&self) -> Vec<T> {
        self.items.clone()
    }
}

pub fn index<T: Copy>(s: &[T], i: usize) -> T {
    s[i]
}

pub fn sum(v: &Vec<u32>) -> u32 {
    let mut acc = 0;
    for x in v.iter() {
        acc += *x;
    }
    acc
}

pub fn array_index(a: [u64; 4], i: usize) -> u64 {
    a[i]
}

pub fn vec_index(v: &Vec<u64>, i: usize) -> u64 {
    v[i]
}

pub fn main() {
    let mut w: Wrapper<Nop> = Wrapper { items: Vec::new() };
    w.push_clone(&Nop { x: 1 });
    let _ = w.first_mapped();
    let _ = w.duplicate();
    let _ = index(&[1u8, 2], 1);
    let _ = sum(&vec![1, 2]);
    let _ = array_index([1, 2, 3, 4], 2);
}
