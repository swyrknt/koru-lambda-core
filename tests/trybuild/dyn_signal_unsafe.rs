// Compile-fail probe: `dyn Signal` is object-unsafe by design.
//
// `Signal::compute_aggregate` takes `impl IntoIterator`, which
// precludes vtable dispatch. Any attempt to construct
// `Box<dyn Signal<Output = usize>>` (or any other `dyn Signal`)
// must fail with an object-safety error at compile time.
//
// See `docs/SPEC.md` §5 for the design rationale.

use koru_lambda_core::projection::Signal;

fn takes_dyn(_: Box<dyn Signal<Output = usize>>) {}

fn main() {}
