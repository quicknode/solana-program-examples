pub mod convert_if_triggered;
pub mod deposit;
pub mod initialize_vault;
pub mod update_threshold;
pub mod withdraw_stables;
pub mod withdraw_volatile;

// Glob-export each instruction module so the `#[program]` macro can resolve
// the `__client_accounts_*` helper modules it generates per instruction.
// We accept the `ambiguous_glob_reexports` warning over `handler` because
// every handler is referenced from `lib.rs` by its full module path
// (`instructions::deposit::handler`, etc.), so the glob ambiguity is never
// actually resolved against a `handler` symbol at the crate root.
#[allow(ambiguous_glob_reexports)]
pub use convert_if_triggered::*;
#[allow(ambiguous_glob_reexports)]
pub use deposit::*;
#[allow(ambiguous_glob_reexports)]
pub use initialize_vault::*;
#[allow(ambiguous_glob_reexports)]
pub use update_threshold::*;
#[allow(ambiguous_glob_reexports)]
pub use withdraw_stables::*;
#[allow(ambiguous_glob_reexports)]
pub use withdraw_volatile::*;
