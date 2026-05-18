//! Mock Switchboard On-Demand feed for testing the stop-loss vault.
//!
//! Real Switchboard On-Demand feeds are program-owned accounts whose data is
//! produced by an off-chain oracle network and verified onchain via Ed25519
//! signatures over the latest price update. That verification path is
//! out-of-scope for this teaching example, so this mock stores a single price
//! the test harness writes directly, plus the slot the update happened in.
//!
//! The on-chain reader (`stop-loss-vault::convert_if_triggered`) reads the
//! mock feed the same way it would read a real feed: load the account, decode
//! the layout, read `price` and `last_update_slot`. Swap this program ID for
//! `SBondMDrcV3K4kxZR1HNVT7osZxAHVHgYXL5Ze1oMUv` (Switchboard On-Demand) and
//! adapt the layout to consume real feeds in production.
//!
//! NOT FOR PRODUCTION.
use anchor_lang::prelude::*;

declare_id!("GAbm8tcMimkhYsQZm24N3Ev1kuWbTKXkTQ1gQEpfJ9Gg");

#[program]
pub mod mock_switchboard {
    use super::*;

    /// Initialise the mock feed with an initial price. The signer becomes the
    /// authority allowed to push later price updates.
    pub fn initialize_feed(
        ctx: Context<InitializeFeed>,
        price: i128,
        scale: u32,
    ) -> Result<()> {
        let feed = &mut ctx.accounts.feed;
        feed.authority = ctx.accounts.authority.key();
        feed.price = price;
        feed.scale = scale;
        feed.last_update_slot = Clock::get()?.slot;
        Ok(())
    }

    /// Push a new price to the mock feed. In real Switchboard this would be a
    /// signed update from the oracle network; here it's just an authority-gated
    /// write, because the goal is to drive deterministic test scenarios.
    pub fn set_price(ctx: Context<SetPrice>, price: i128) -> Result<()> {
        let feed = &mut ctx.accounts.feed;
        feed.price = price;
        feed.last_update_slot = Clock::get()?.slot;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitializeFeed<'info> {
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
pub struct SetPrice<'info> {
    #[account(
        mut,
        has_one = authority,
    )]
    pub feed: Account<'info, MockFeed>,

    pub authority: Signer<'info>,
}

/// Mock of a Switchboard On-Demand feed. Real feeds carry many more fields
/// (median, range, sample window, signatures) — this is the bare minimum the
/// vault needs to do a price comparison.
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
}
