//@ no-default-options
//@ charon-args=--preset=aeneas --monomorphize --devirtualize=test_crate::Isa=test_crate::Backend
//@ rustc-args=--edition=2021
//! `--devirtualize` with `--monomorphize`: impls don't list their methods in mono mode, so the
//! former vtable calls must still become direct calls to the (monomorphic) impl methods, and the
//! vtable must disappear, leaving no recursive group (the `TargetIsa`/`compile_function` shape).
pub trait Isa {
    fn name(&self) -> u32;
    fn compile(&self, x: u32) -> u32;
}

pub struct Backend(pub u32);

impl Isa for Backend {
    fn name(&self) -> u32 {
        self.0
    }
    fn compile(&self, x: u32) -> u32 {
        // Calls back into code that takes `&dyn Isa`.
        if x == 0 { 0 } else { lower(self, x - 1) + 1 }
    }
}

pub struct Ctx<T> {
    pub v: T,
}

fn lower(isa: &dyn Isa, x: u32) -> u32 {
    let c = Ctx { v: x };
    isa.compile(c.v) + isa.name()
}

pub fn run() -> u32 {
    let b = Backend(7);
    let boxed: Box<dyn Isa> = Box::new(Backend(1));
    lower(&b, 3) + boxed.name()
}
