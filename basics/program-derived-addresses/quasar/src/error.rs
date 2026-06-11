use quasar_lang::prelude::*;

#[error_code]
pub enum PageVisitsError {
    /// The visit count is at its maximum and cannot be incremented further.
    // 6000 is the conventional Anchor-compatible starting offset for
    // program-specific error codes (Quasar's #[error_code] starts at 0
    // unless told otherwise; framework errors occupy 3000+).
    MathOverflow = 6000,
}
