use quasar_lang::prelude::*;

pub const REGISTRY_SEED: &[u8] = b"registry";
pub const APPROVED_ASSET_SEED: &[u8] = b"approved_asset";

/// The curator record at the root of an approved-asset set. This account holds
/// no list: it only names the authority allowed to approve assets under it. The
/// set itself is the collection of `ApprovedAsset` accounts derived from this
/// registry, and an asset counts as approved exactly when its account exists.
///
/// The authority is deliberately not a strategy manager: the curator vets which
/// real assets (and which official price feed) are safe, and a manager only
/// chooses among them. This is what stops a manager from listing a token they
/// mint themselves, or pairing a real mint with a feed they control.
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
/// PDA: `["approved_asset", registry, mint]`.
#[account(discriminator = 2, set_inner)]
#[seeds(b"approved_asset", registry: Address, mint: Address)]
pub struct ApprovedAsset {
    pub registry: Address,
    pub mint: Address,
    pub price_feed: Address,
    pub bump: u8,
}
