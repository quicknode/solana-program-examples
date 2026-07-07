use quasar_lang::prelude::*;

pub const REGISTRY_SEED: &[u8] = b"registry";
pub const WHITELIST_SEED: &[u8] = b"whitelist";

/// A curated set of assets that strategies may hold, maintained by a protocol
/// authority that is deliberately not the strategy manager: the authority vets
/// which real assets (and which official price feed) are safe, and the manager
/// only chooses among them. This is what stops a manager from listing a token
/// they mint themselves, or pairing a real mint with a feed they control.
///
/// PDA: `["registry", authority]`.
#[account(discriminator = 1, set_inner)]
#[seeds(b"registry", authority: Address)]
pub struct Registry {
    pub authority: Address,
    pub bump: u8,
}

/// One approved mint, binding it to its official price feed. Created only by the
/// registry authority; `add_asset` copies `price_feed` from here so the manager
/// never supplies the feed.
///
/// PDA: `["whitelist", registry, mint]`.
#[account(discriminator = 2, set_inner)]
#[seeds(b"whitelist", registry: Address, mint: Address)]
pub struct WhitelistEntry {
    pub registry: Address,
    pub mint: Address,
    pub price_feed: Address,
    pub bump: u8,
}
