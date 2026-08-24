use anchor_lang::prelude::*;

mod instructions;
use instructions::*;

declare_id!("Fod47xKXjdHVQDzkFPBvfdWLm8gEAV4iMSXkfUzCHiSD");

#[program]
pub mod anchor_realloc {
    use super::*;

    pub fn initialize(context: Context<InitializeAccountConstraints>, input: String) -> Result<()> {
        instructions::initialize::handler(context, input)
    }

    pub fn update(context: Context<UpdateAccountConstraints>, input: String) -> Result<()> {
        instructions::update::handler(context, input)
    }
}

// NOTE: We intentionally do NOT derive `InitSpace` on `Message`. The realloc
// example demonstrates growing/shrinking the account to exactly fit the
// caller-provided `input`, whose length isn't known at compile time.
// `InitSpace` + `#[max_len(N)]` would force a fixed upper bound, defeating
// the point of the example. Instead, `required_space` computes the exact
// layout (discriminator + length prefix + bytes) for init/realloc.
#[account]
pub struct Message {
    pub message: String,
}

impl Message {
    pub fn required_space(input_len: usize) -> usize {
        8 + // 8 byte discriminator
        4 + // 4 byte for length of string
        input_len
    }
}
