use anchor_lang::prelude::*;

// `borsh` because the struct holds a `String`: v2's default `#[account]`
// backing is zero-copy and needs a `Pod` (fixed-layout) type.
#[account(borsh)]
#[derive(InitSpace)] // automatically calculate the space required for the struct
pub struct User {
    pub bump: u8,      // 1 byte
    pub user: Address, // 32 bytes
    #[max_len(50)] // set a max length for the string
    pub name: String, // 4 bytes + 50 bytes
}
