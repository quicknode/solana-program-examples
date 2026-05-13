// Test-side helpers. Split into modules so each file has a single
// responsibility and the test files stay focused on behaviour, not plumbing.
//
// Each integration test compiles `common/` into a separate binary, so an
// item used by one test binary but not another shows up as `dead_code`. The
// allow attribute below silences those false positives across the whole
// helper surface; real dead code is still caught by `cargo clippy`.
#![allow(dead_code)]

pub mod squads;
pub mod world;
