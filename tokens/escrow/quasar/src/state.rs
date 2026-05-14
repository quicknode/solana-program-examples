use quasar_lang::prelude::*;

/// Offer state: records the maker's desired receive amount and the
/// associated mint/token-account addresses.
#[account(discriminator = 1, set_inner)]
#[seeds(b"offer", maker: Address)]
pub struct Offer {
    pub maker: Address,
    pub token_mint_a: Address,
    pub token_mint_b: Address,
    pub maker_token_account_b: Address,
    pub receive: u64,
    pub bump: u8,
}
