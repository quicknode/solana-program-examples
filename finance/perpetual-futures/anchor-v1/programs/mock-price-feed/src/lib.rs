//! Mock oracle price feed for testing the perpetual-futures program.
//!
//! A real price feed account is written by an oracle network's receiver
//! program, which verifies the network's signatures over each price update
//! before recording it. That verification path is out of scope for this
//! teaching example, so this mock stores a single price the test harness
//! writes directly, plus the slot the update happened in.
//!
//! The perpetual-futures program reads this feed the same way it would read a real
//! feed: load the account, decode the layout, read `price`, `scale`, and
//! `last_update_slot` (see `perpetual_futures::state::oracle`). In production the
//! program reads a Pyth `PriceUpdateV2` account instead, owned by the Pyth
//! Receiver program `rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ`. The oracle
//! module describes that change, and `basics/pyth` reads one.
//!
//! NOT FOR PRODUCTION.
use anchor_lang::prelude::*;

declare_id!("FnisQqhF56BxVYh5Wt8xW8wuTVN6STAGnk13MM5SRM7b");

#[program]
pub mod mock_price_feed {
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

    /// Push a new price (and confidence band) to the mock feed. For a real
    /// feed this would be a signed update from the oracle network; here it is
    /// an authority-gated write, because the goal is to drive deterministic
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

/// Mock of an oracle price feed. Real feeds carry many more fields (feed ID,
/// publish time, EMA price, verification level) — this is the bare minimum the
/// perpetual-futures program needs to do a price comparison.
#[derive(InitSpace)]
#[account]
pub struct MockFeed {
    pub authority: Pubkey,

    /// Signed 128-bit fixed-point price, wide enough for any feed's price
    /// (Pyth's is an i64 with a separate exponent).
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
