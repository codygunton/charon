//@ known-failure
//@ charon-args=--devirtualize=test_crate::Shape=test_crate::Square
//! `Circle` is also unsized to `dyn Shape`, so `dyn Shape` can't be replaced by `Square`.
pub trait Shape {
    fn area(&self) -> u32;
}

pub struct Square(pub u32);
pub struct Circle(pub u32);

impl Shape for Square {
    fn area(&self) -> u32 {
        self.0 * self.0
    }
}

impl Shape for Circle {
    fn area(&self) -> u32 {
        3 * self.0 * self.0
    }
}

fn total(s: &dyn Shape) -> u32 {
    s.area()
}

pub fn run() -> u32 {
    total(&Square(2)) + total(&Circle(1))
}
