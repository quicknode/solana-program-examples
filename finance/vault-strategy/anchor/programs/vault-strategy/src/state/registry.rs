use anchor_lang::prelude::*;

/// A curated set of assets that strategies may hold. Maintained by a protocol
/// authority that is deliberately not the strategy manager: the authority vets
/// which real assets (and which official price feed) are safe, and the manager
/// only chooses among them. This is what stops a manager from listing a token
/// they mint themselves, or pairing a real mint with a feed they control.
#[account]
#[derive(InitSpace)]
pub struct Registry {
    pub authority: Pubkey,
    pub bump: u8,
}

/// One approved mint, binding it to its official Pyth PriceUpdateV2 feed.
/// Created only by the registry authority; add_asset copies `price_feed` from
/// here so the manager never supplies the feed.
#[account]
#[derive(InitSpace)]
pub struct WhitelistEntry {
    pub registry: Pubkey,
    pub mint: Pubkey,
    pub price_feed: Pubkey,
    pub bump: u8,
}
