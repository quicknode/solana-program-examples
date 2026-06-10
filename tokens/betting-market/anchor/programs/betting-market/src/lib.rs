pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use instructions::*;
pub use state::*;

declare_id!("7LyqAeLR3mK9dfj9LqxWzfKH61VVHzuNpkgW5Y32De74");

#[program]
pub mod betting_market {
    use super::*;

    // One-time setup: the signer becomes the admin and fixes the stake token and
    // the settlement fee (basis points) for every market in this deployment.
    pub fn initialize_config(
        context: Context<InitializeConfig>,
        fee_bps: u16,
        fee_recipient: Pubkey,
    ) -> Result<()> {
        instructions::initialize_config::handle_initialize_config(context, fee_bps, fee_recipient)
    }

    // Admin opens a new market and creates its pool vault.
    pub fn create_event(
        context: Context<CreateEvent>,
        event_id: u64,
        description: String,
    ) -> Result<()> {
        instructions::create_event::handle_create_event(context, event_id, description)
    }

    // Admin adds a possible result. Only allowed before betting starts.
    pub fn add_outcome(context: Context<AddOutcome>, label: String) -> Result<()> {
        instructions::add_outcome::handle_add_outcome(context, label)
    }

    // A bettor stakes tokens on one outcome. The stake joins the event's pool.
    pub fn place_bet(context: Context<PlaceBetAccountConstraints>, amount: u64) -> Result<()> {
        instructions::place_bet::handle_place_bet(context, amount)
    }

    // Admin resolves the market: takes the fee from the losing pool and records
    // the figures winners need to claim their share.
    pub fn settle_event(context: Context<SettleEvent>, winning_outcome_index: u8) -> Result<()> {
        instructions::settle_event::handle_settle_event(context, winning_outcome_index)
    }

    // A winner withdraws their stake plus their pro-rata share of the losing
    // pool. The Bet account closes and leaves the bettor's User index.
    pub fn claim_winnings(context: Context<ClaimWinningsAccountConstraints>) -> Result<()> {
        instructions::claim_winnings::handle_claim_winnings(context)
    }

    // A loser closes their worthless bet after settlement, reclaiming the
    // Bet account's rent and freeing the slot in their User index.
    pub fn close_losing_bet(context: Context<CloseLosingBetAccountConstraints>) -> Result<()> {
        instructions::close_losing_bet::handle_close_losing_bet(context)
    }

    // Admin voids an unresolved market so bettors can be made whole.
    pub fn cancel_event(context: Context<CancelEvent>) -> Result<()> {
        instructions::cancel_event::handle_cancel_event(context)
    }

    // After a cancellation, a bettor reclaims their exact stake. The Bet
    // account closes and leaves the bettor's User index.
    pub fn claim_refund(context: Context<ClaimRefundAccountConstraints>) -> Result<()> {
        instructions::claim_refund::handle_claim_refund(context)
    }
}
