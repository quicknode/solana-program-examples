use quasar_lang::prelude::*;

pub const ROUTER_CONFIG_SEED: &[u8] = b"router_config";
pub const ASSET_RATE_SEED: &[u8] = b"rate";

/// Router configuration. PDA: `["router_config"]`. A single deployment-wide
/// account naming the authority (who may set rates) and the base USDC mint.
/// It is also the authority of the USDC treasury and the mint authority of
/// every asset mint the router mints, and signs those CPIs with its own seeds.
#[account(discriminator = 1, set_inner)]
#[seeds(b"router_config")]
pub struct RouterConfig {
    pub authority: Address,
    pub usdc_mint: Address,
    pub bump: u8,
}

/// A fixed price for one asset. PDA: `["rate", mint]`.
#[account(discriminator = 2, set_inner)]
#[seeds(b"rate", mint: Address)]
pub struct AssetRate {
    pub mint: Address,
    /// USDC base units per whole token, e.g. 250_000_000 means 1.0 TSLAx = $250
    /// with six-decimal USDC. The swaps scale by the asset mint's decimals, so the
    /// rate means the same thing whatever precision the asset has.
    pub usdc_per_token: u64,
    pub bump: u8,
}

/// PDA token account (USDC) the router pays out of and collects into:
/// `["treasury"]`, authority = RouterConfig.
#[derive(Seeds)]
#[seeds(b"treasury")]
pub struct TreasuryPda;
