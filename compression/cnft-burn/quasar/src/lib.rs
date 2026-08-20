#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

pub mod instructions;
use instructions::*;
#[cfg(test)]
mod tests;

/// Bubblegum Burn instruction discriminator.
const BURN_DISCRIMINATOR: [u8; 8] = [116, 110, 29, 56, 107, 219, 42, 93];

/// mpl-bubblegum program ID (BGUMAp9Gq7iTEuizy4pqaxsTyUCBK68MDfK752saRPUY).
const MPL_BUBBLEGUM_ID: Address = Address::new_from_array([
    0x98, 0x8b, 0x80, 0xeb, 0x79, 0x35, 0x28, 0x69, 0xb2, 0x24, 0x74, 0x5f, 0x59, 0xdd, 0xbf, 0x8a,
    0x26, 0x58, 0xca, 0x13, 0xdc, 0x68, 0x81, 0x21, 0x26, 0x35, 0x1c, 0xae, 0x07, 0xc1, 0xa5, 0xa5,
]);

/// SPL Account Compression program ID (cmtDvXumGCrqC1Age74AVPhSRVXJMd8PJS91L8KbNCK).
const SPL_ACCOUNT_COMPRESSION_ID: Address = Address::new_from_array([
    0x09, 0x2a, 0x13, 0xee, 0x95, 0xc4, 0x1c, 0xba, 0x08, 0xa6, 0x7f, 0x5a, 0xc6, 0x7e, 0x8d, 0xf7,
    0xe1, 0xda, 0x11, 0x62, 0x5e, 0x1d, 0x64, 0x13, 0x7f, 0x8f, 0x4f, 0x23, 0x83, 0x03, 0x7f, 0x14,
]);

declare_id!("C6qxH8n6mZxrrbtMtYWYSp8JR8vkQ55X1o4EBg7twnMv");

#[program]
mod quasar_cnft_burn {
    use super::*;

    /// The Bubblegum Burn args arrive as typed instruction arguments: 0.1.0
    /// clears `ctx.data` after decoding declared args (the instruction
    /// argument zero-copy boundary), so the pre-0.1.0 raw-tail pattern reads
    /// an empty slice. Only the variable-length proof stays dynamic, as
    /// remaining accounts.
    #[instruction(discriminator = 0)]
    pub fn burn_cnft(
        ctx: CtxWithRemaining<BurnCnftAccountConstraints>,
        root: [u8; 32],
        data_hash: [u8; 32],
        creator_hash: [u8; 32],
        nonce: u64,
        index: u32,
    ) -> Result<(), ProgramError> {
        let remaining = ctx.remaining_accounts();
        instructions::handle_burn_cnft(
            &mut ctx.accounts,
            root,
            data_hash,
            creator_hash,
            nonce,
            index,
            remaining,
        )
    }
}
