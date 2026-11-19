//@ no-default-options
//@ charon-args=--preset=aeneas --monomorphize-local-only
//@ rustc-args=--edition=2021
//! Monomorphic instances whose type arguments contain references bind the regions of these
//! arguments as region parameters (one per occurrence), instead of erasing them: e.g.
//! `Holder::<&'a u32><'a>`, and `mk::<&'a u32><'a>(x: &'a u32) -> Holder::<&'a u32><'a>`.
//! Uses pass the corresponding regions.

pub struct Ctx<'a> {
    pub v: &'a mut Vec<u32>,
}

pub struct Holder<T> {
    pub x: T,
}

impl<T> Holder<T> {
    pub fn get(&self) -> &T {
        &self.x
    }
}

pub fn mk<T>(x: T) -> Holder<T> {
    Holder { x }
}

pub trait Push {
    fn push(&mut self, x: u32);
}

impl<'a> Push for Ctx<'a> {
    fn push(&mut self, x: u32) {
        self.v.push(x)
    }
}

pub fn push_all<C: Push>(c: &mut C, n: u32) {
    let mut i = 0;
    while i < n {
        c.push(i);
        i += 1;
    }
}

pub fn wrap<'a>(c: Ctx<'a>) -> Holder<Ctx<'a>> {
    mk(c)
}

pub fn pair<'a, 'b>(x: &'a u32, y: &'b mut u32) -> Holder<(&'a u32, &'b mut u32)> {
    mk((x, y))
}

pub fn main() {
    let mut v = Vec::new();
    let mut c = Ctx { v: &mut v };
    push_all(&mut c, 3);
    let h = wrap(c);
    let _ = h.get();
    let x = 5u32;
    let h = mk(&x);
    let _ = *h.get();
    let mut y = 7u32;
    let _ = pair(&x, &mut y);
}
