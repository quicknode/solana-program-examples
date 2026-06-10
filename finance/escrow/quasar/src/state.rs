use quasar_lang::prelude::*;

/// Offer state: records the maker's desired receive amount and the
/// associated mint/token-account addresses. The `id` seed lets one maker
/// keep multiple offers open at once (matching the Anchor variant's
/// `["offer", maker, id]` seeds).
#[account(discriminator = 1, set_inner)]
#[seeds(b"offer", maker: Address, id: u64)]
pub struct Offer {
    pub id: u64,
    pub maker: Address,
    pub token_mint_a: Address,
    pub token_mint_b: Address,
    pub maker_token_account_b: Address,
    pub vault: Address,
    pub receive: u64,
    pub bump: u8,
}
