//@ revisions=dup,limited
//@[limited] charon-args=--code-duplication-limit 1
// `--code-duplication-limit N`: control-flow reconstruction duplicates the blocks reachable from
// several branches (`dup`), unless that translates more than N times the body's block count, in
// which case the body is reconstructed with forward `break`s instead (`limited`), as with
// `--no-code-duplication`.
fn shared_tail(a: bool, b: bool, c: bool) -> u32 {
    let mut x = 0;
    if a && b || c {
        x += 1;
    } else {
        x += 2;
    }
    if (a || b) && c {
        x += 3;
    } else {
        x += 4;
    }
    x
}

// Small enough to stay under the limit: reconstructed as usual in both revisions.
fn straight(a: bool) -> u32 {
    if a { 1 } else { 2 }
}
