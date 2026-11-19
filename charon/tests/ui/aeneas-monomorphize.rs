//@ no-default-options
//@ charon-args=--preset=aeneas --monomorphize
//@ rustc-args=--edition=2021
//! The `MachBuffer<I: VCodeInst>` shape: a generic struct parameterized by a trait whose impl
//! refers back to the struct. Polymorphically this is one mixed type/trait/function group;
//! monomorphized it must become plain, non-generic declarations.

pub struct Buf<I: Inst> {
    v: Vec<I>,
    pos: (u32, bool),
}

impl<I: Inst> Buf<I> {
    pub fn push(&mut self, i: I) {
        self.v.push(i);
        self.pos.0 += 1;
    }
    pub fn first_len(&self, xs: &[u8; 4]) -> usize {
        let s: &[u8] = xs;
        s[1..].len() + self.v.len()
    }
}

pub trait Inst {
    fn emit(&self, b: &mut Buf<Self>)
    where
        Self: Sized;
}

#[derive(Clone, Copy)]
pub struct Nop(pub u8);

impl Inst for Nop {
    fn emit(&self, b: &mut Buf<Nop>) {
        if self.0 > 0 {
            Nop(self.0 - 1).emit(b);
        }
        b.push(*self);
    }
}

pub fn run() -> usize {
    let mut b = Buf {
        v: Vec::new(),
        pos: (0, false),
    };
    Nop(2).emit(&mut b);
    let mut arr = [0u8; 4];
    arr[1] = 3;
    let x = arr[2] as usize;
    let boxed = Box::new(Nop(5));
    b.first_len(&arr) + x + boxed.0 as usize
}
