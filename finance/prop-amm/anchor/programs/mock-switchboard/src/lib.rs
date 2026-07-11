//! Mock Switchboard On-Demand feed for testing the prop-amm program.
//!
//! Real Switchboard On-Demand feeds are program-owned accounts whose data is
//! produced by an offchain oracle network and verified onchain via Ed25519
//! signatures over the latest price update. That verification path is
//! out-of-scope for this teaching example, so this mock stores a single price
//! the test harness writes directly, plus the slot the update happened in.
//!
//! The prop-amm program reads this feed the same way it would read a real
//! feed: load the account, decode the layout, read `price`, `scale`, and
//! `last_update_slot` (see `prop_amm::state::oracle`). Swap this program ID
//! for `SBondMDrcV3K4kxZR1HNVT7osZxAHVHgYXL5Ze1oMUv` (Switchboard On-Demand)
//! and adapt the layout to consume real feeds in production.
//!
//! NOT FOR PRODUCTION.
use anchor_lang::prelude::*;

declare_id!("BdFYarCfgMJhC26dFN6JXKVx4kUEM3k2wYaNjyjFrvAD");

#[program]
pub mod mock_switchboard {
    use super::*;

    /// Initialize the mock feed with an initial price. The signer becomes the
    /// authority allowed to push later price updates.
    pub fn initialize_feed(
        context: Context<InitializeFeedAccountConstraints>,
        price: i128,
        scale: u32,
        confidence: u64,
    ) -> Result<()> {
        let feed = &mut context.accounts.feed;
        feed.authority = context.accounts.authority.key();
        feed.price = price;
        feed.scale = scale;
        feed.last_update_slot = Clock::get()?.slot;
        feed.confidence = confidence;
        Ok(())
    }

    /// Push a new price (and confidence band) to the mock feed. In real
    /// Switchboard this would be a signed update from the oracle network; here it
    /// is an authority-gated write, because the goal is to drive deterministic
    /// test scenarios.
    pub fn set_price(
        context: Context<SetPriceAccountConstraints>,
        price: i128,
        confidence: u64,
    ) -> Result<()> {
        let feed = &mut context.accounts.feed;
        feed.price = price;
        feed.last_update_slot = Clock::get()?.slot;
        feed.confidence = confidence;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitializeFeedAccountConstraints<'info> {
    #[account(
        init,
        payer = authority,
        space = MockFeed::DISCRIMINATOR.len() + MockFeed::INIT_SPACE,
    )]
    pub feed: Account<'info, MockFeed>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SetPriceAccountConstraints<'info> {
    #[account(
        mut,
        has_one = authority,
    )]
    pub feed: Account<'info, MockFeed>,

    pub authority: Signer<'info>,
}

/// Mock of a Switchboard On-Demand feed. Real feeds carry many more fields
/// (median, range, sample window, signatures) — this is the bare minimum the
/// prop-amm program needs to price a quote.
#[derive(InitSpace)]
#[account]
pub struct MockFeed {
    pub authority: Pubkey,

    /// Signed 128-bit fixed-point price. Real Switchboard prices are also i128.
    pub price: i128,

    /// Number of decimal places implied by `price`. E.g. `scale = 8` means
    /// `price = 200 * 10^8` represents $200.00000000.
    pub scale: u32,

    pub last_update_slot: u64,

    /// Uncertainty band around `price`, in the same fixed point. Real feeds
    /// report a standard-deviation-like confidence; consumers reject the price
    /// when this is too wide relative to `price`.
    pub confidence: u64,
}
