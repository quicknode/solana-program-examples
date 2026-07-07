use quasar_lang::prelude::*;

pub const ROUTER_CONFIG_SEED: &[u8] = b"router_config";
pub const ASSET_RATE_SEED: &[u8] = b"rate";
pub const ROUTER_AUTHORITY_SEED: &[u8] = b"router_authority";

/// Router configuration. PDA: `["router_config"]`. A single deployment-wide
/// account naming the authority (who may set rates) and the base USDC mint.
#[account(discriminator = 1, set_inner)]
#[seeds(b"router_config")]
pub struct RouterConfig {
    pub authority: Address,
    pub usdc_mint: Address,
    pub bump: u8,
}

/// A fixed price for one asset. PDA: `["rate", mint]`. `usdc_per_token` is USDC
/// base units per token base unit (e.g. 250 means 1 token = 250 USDC when both
/// have 6 decimals).
#[account(discriminator = 2, set_inner)]
#[seeds(b"rate", mint: Address)]
pub struct AssetRate {
    pub mint: Address,
    pub usdc_per_token: u64,
    pub bump: u8,
}

/// PDA that is the mint authority of every asset mint and the authority of the
/// USDC treasury: `["router_authority"]`.
#[derive(Seeds)]
#[seeds(b"router_authority")]
pub struct RouterAuthorityPda;

/// PDA token account (USDC) the router pays out of and collects into:
/// `["treasury"]`, authority = RouterAuthorityPda.
#[derive(Seeds)]
#[seeds(b"treasury")]
pub struct TreasuryPda;
