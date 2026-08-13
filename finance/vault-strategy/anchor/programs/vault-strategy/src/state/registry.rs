use anchor_lang::prelude::*;

/// The curator record at the root of an approved-asset set. This account holds no
/// list: it only names the authority allowed to approve assets under it. The set
/// itself is the collection of `ApprovedAsset` accounts derived from this registry,
/// and an asset counts as approved exactly when its `ApprovedAsset` account exists.
///
/// The authority is deliberately not a strategy manager: the curator vets which
/// real assets (and which official price feed) are safe, and a manager only
/// chooses among them. This is what stops a manager from listing a token they
/// mint themselves, or pairing a real mint with a feed they control.
#[account(borsh)]
#[derive(InitSpace)]
pub struct Registry {
    pub authority: Address,
    pub bump: u8,
}

/// One curator-approved asset, binding its mint to its official Pyth
/// PriceUpdateV2 feed. Its address is a PDA seeded by the registry and the mint,
/// so existence of this account is the approval check: add_asset derives the
/// address and fails if no account is there. Created only by the registry
/// authority; add_asset copies `price_feed` from here so the manager never
/// supplies the feed.
#[account(borsh)]
#[derive(InitSpace)]
pub struct ApprovedAsset {
    pub registry: Address,
    pub mint: Address,
    pub price_feed: Address,
    pub bump: u8,
}
