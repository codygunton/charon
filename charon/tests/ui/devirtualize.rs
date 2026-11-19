//@ charon-args=--devirtualize=test_crate::Shape=test_crate::Square
//! `dyn Shape` has a single implementation, `Square`: the trait object type becomes `Square`, the
//! unsizing casts become moves and the vtable calls become direct calls.
pub trait Shape {
    fn area(&self) -> u32;
    fn scale(&mut self, k: u32);
}

pub struct Square(pub u32);

impl Shape for Square {
    fn area(&self) -> u32 {
        self.0 * self.0
    }
    fn scale(&mut self, k: u32) {
        self.0 *= k;
    }
}

fn total(s: &dyn Shape) -> u32 {
    s.area() + 1
}

fn grow(s: &mut dyn Shape) {
    s.scale(2)
}

pub fn run() -> u32 {
    let mut sq = Square(3);
    grow(&mut sq);
    let b: Box<dyn Shape> = Box::new(Square(1));
    total(&sq) + b.area()
}
