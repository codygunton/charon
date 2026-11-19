//@ charon-args=--no-layouts --print-layouts
// `--no-layouts`: type layouts are not translated (`TypeDecl::layout` stays empty).
struct Pair {
    a: u32,
    b: u8,
}

fn first(p: Pair) -> u32 {
    p.a
}
