use pinocchio::pubkey::Pubkey;

use super::{Discriminator, Transmutable};


// `#[repr(C, packed)]` keeps the on-chain layout exactly 41 bytes wide.
// With plain `#[repr(C)]` the u64 field gets 7 bytes of alignment padding
// inserted after the 33-byte (u8 + Pubkey) prefix, making the struct 48 bytes
// while `LEN = 41`. The program would then read 7 bytes past the end of the
// account buffer and fault with `AccountDataTooSmall`. Packed avoids that and
// matches the byte layout the SDK and tests expect.
#[repr(C, packed)]
pub struct Config {
    pub discriminator: u8,
    pub authority: Pubkey,
    pub blocked_wallets_count: u64,
}

impl Config {
    pub const SEED_PREFIX: &'static [u8] = b"config";
}

impl Transmutable for Config {
    const LEN: usize = 1 + 32 + 8;
}

impl Discriminator for Config {
    const DISCRIMINATOR: u8 = 0x01;
}
