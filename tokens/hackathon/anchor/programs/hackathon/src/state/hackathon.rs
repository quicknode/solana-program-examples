use anchor_lang::prelude::*;

// Upper bound on the length of `Hackathon.name`. Picked to keep the account
// small while still allowing reasonable human-readable names. Names longer
// than this should be hashed offchain before being passed in.
pub const HACKATHON_NAME_MAX_LEN: usize = 64;

#[account]
#[derive(InitSpace)]
pub struct Hackathon {
    // The "admin" key. In practice this is a Squads vault PDA, but the
    // program treats it as an opaque pubkey: privileged handlers check
    // `signer == authority` and nothing more.
    pub authority: Pubkey,
    // SHA-256 of `name`, stored on the account so other instructions can
    // reference it directly in their `seeds = [...]` constraints without
    // calling a function on `name` (Anchor's IDL builder does not support
    // function calls in seed expressions). Bound to `name` by the
    // `create_hackathon` handler.
    pub name_seed: [u8; 32],
    // Monotonic counter used to seed Prize PDAs. u8 caps at 255 prizes per
    // hackathon, which is plenty for the target use case (30-100 prizes).
    pub prize_count: u8,
    pub bump: u8,
    // Free-form human-readable name. Bound into the Hackathon PDA via
    // `name_seed` so a single authority can run multiple hackathons.
    #[max_len(HACKATHON_NAME_MAX_LEN)]
    pub name: String,
}
