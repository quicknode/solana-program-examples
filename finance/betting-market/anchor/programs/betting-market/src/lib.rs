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
    // the default settlement fee (basis points) that each new market copies at
    // creation.
    pub fn initialize_config(
        context: &mut Context<InitializeConfigAccountConstraints>,
        default_fee_bps: u16,
        fee_recipient: Address,
    ) -> Result<()> {
        instructions::initialize_config::handle_initialize_config(
            context,
            default_fee_bps,
            fee_recipient,
        )
    }

    // Admin creates a new market as a draft, fixes when betting closes, and
    // creates its pool vault.
    pub fn initialize_event(
        context: &mut Context<InitializeEventAccountConstraints>,
        event_id: u64,
        betting_closes_at: i64,
        description: String,
    ) -> Result<()> {
        instructions::initialize_event::handle_initialize_event(
            context,
            event_id,
            betting_closes_at,
            description,
        )
    }

    // Admin adds a possible result. Only allowed while the event is a draft.
    pub fn add_outcome(
        context: &mut Context<AddOutcomeAccountConstraints>,
        label: String,
    ) -> Result<()> {
        instructions::add_outcome::handle_add_outcome(context, label)
    }

    // Admin finalizes the outcome list and opens the market to bets. Needs at
    // least two outcomes.
    pub fn open_betting(context: &mut Context<OpenBettingAccountConstraints>) -> Result<()> {
        instructions::open_betting::handle_open_betting(context)
    }

    // A bettor stakes tokens on one outcome, before betting closes. The stake
    // joins the event's pool.
    pub fn place_bet(context: &mut Context<PlaceBetAccountConstraints>, amount: u64) -> Result<()> {
        instructions::place_bet::handle_place_bet(context, amount)
    }

    // Admin resolves the market once betting has closed: takes the fee from the losing pool and records
    // the figures winners need to claim their share.
    pub fn settle_event(
        context: &mut Context<SettleEventAccountConstraints>,
        winning_outcome_index: u8,
    ) -> Result<()> {
        instructions::settle_event::handle_settle_event(context, winning_outcome_index)
    }

    // A winner withdraws their stake plus their pro-rata share of the losing
    // pool. The Bet account closes and leaves the bettor's User index.
    pub fn claim_winnings(context: &mut Context<ClaimWinningsAccountConstraints>) -> Result<()> {
        instructions::claim_winnings::handle_claim_winnings(context)
    }

    // A loser closes their worthless bet after settlement, reclaiming the
    // Bet account's rent and freeing the slot in their User index.
    pub fn close_losing_bet(context: &mut Context<CloseLosingBetAccountConstraints>) -> Result<()> {
        instructions::close_losing_bet::handle_close_losing_bet(context)
    }

    // Admin voids a draft or unresolved market so bettors can be made whole.
    pub fn cancel_event(context: &mut Context<CancelEventAccountConstraints>) -> Result<()> {
        instructions::cancel_event::handle_cancel_event(context)
    }

    // After a cancellation, a bettor reclaims their exact stake. The Bet
    // account closes and leaves the bettor's User index.
    pub fn claim_refund(context: &mut Context<ClaimRefundAccountConstraints>) -> Result<()> {
        instructions::claim_refund::handle_claim_refund(context)
    }
}
