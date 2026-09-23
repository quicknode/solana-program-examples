#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

pub mod errors;
pub mod instructions;
pub mod state;

use instructions::*;

#[cfg(test)]
mod tests;

declare_id!("7LyqAeLR3mK9dfj9LqxWzfKH61VVHzuNpkgW5Y32De74");

/// Parimutuel betting market. An admin creates events, adds outcomes, opens
/// them to bets, and settles or cancels them; bettors stake a fixed token on an outcome, and winners
/// share the losing pool (net of a protocol fee) pro-rata to their stake. See
/// README.md for the full walkthrough.
#[program]
mod quasar_betting_market {
    use super::*;

    /// One-time setup: the signer becomes the admin and fixes the stake token
    /// and the default settlement fee (basis points) that each new market
    /// copies at creation.
    #[instruction(discriminator = 0)]
    pub fn initialize_config(
        ctx: Ctx<InitializeConfigAccountConstraints>,
        default_fee_bps: u16,
        fee_recipient: Address,
    ) -> Result<(), ProgramError> {
        instructions::initialize_config::handle_initialize_config(
            &mut ctx.accounts,
            default_fee_bps,
            fee_recipient,
            &ctx.bumps,
        )
    }

    /// Admin creates a new market as a draft, fixes when betting closes, and
    /// creates its pool vault.
    #[instruction(discriminator = 1)]
    pub fn initialize_event(
        ctx: Ctx<InitializeEventAccountConstraints>,
        event_id: u64,
        betting_closes_at: i64,
        description: String<200>,
    ) -> Result<(), ProgramError> {
        instructions::initialize_event::handle_initialize_event(
            &mut ctx.accounts,
            event_id,
            betting_closes_at,
            description,
            &ctx.bumps,
        )
    }

    /// Admin adds a possible result. Only allowed while the event is a draft.
    #[instruction(discriminator = 2)]
    pub fn add_outcome(
        ctx: Ctx<AddOutcomeAccountConstraints>,
        label: String<64>,
    ) -> Result<(), ProgramError> {
        instructions::add_outcome::handle_add_outcome(&mut ctx.accounts, label, &ctx.bumps)
    }

    /// A bettor stakes tokens on one outcome, before betting closes. The stake
    /// joins the event's pool.
    #[instruction(discriminator = 3)]
    pub fn place_bet(
        ctx: Ctx<PlaceBetAccountConstraints>,
        amount: u64,
    ) -> Result<(), ProgramError> {
        instructions::place_bet::handle_place_bet(&mut ctx.accounts, amount, &ctx.bumps)
    }

    /// Admin resolves the market once betting has closed: takes the fee from the losing pool and records
    /// the figures winners need to claim their share.
    #[instruction(discriminator = 4)]
    pub fn settle_event(
        ctx: Ctx<SettleEventAccountConstraints>,
        winning_outcome_index: u8,
    ) -> Result<(), ProgramError> {
        instructions::settle_event::handle_settle_event(&mut ctx.accounts, winning_outcome_index)
    }

    /// A winner withdraws their stake plus their pro-rata share of the losing
    /// pool. The Bet account closes and leaves the bettor's User index.
    #[instruction(discriminator = 5)]
    pub fn claim_winnings(ctx: Ctx<ClaimWinningsAccountConstraints>) -> Result<(), ProgramError> {
        instructions::claim_winnings::handle_claim_winnings(&mut ctx.accounts)
    }

    /// A loser closes their worthless bet after settlement, reclaiming the Bet
    /// account's rent and freeing the slot in their User index.
    #[instruction(discriminator = 6)]
    pub fn close_losing_bet(
        ctx: Ctx<CloseLosingBetAccountConstraints>,
    ) -> Result<(), ProgramError> {
        instructions::close_losing_bet::handle_close_losing_bet(&mut ctx.accounts)
    }

    /// Admin voids a draft or unresolved market so bettors can be made whole.
    #[instruction(discriminator = 7)]
    pub fn cancel_event(ctx: Ctx<CancelEventAccountConstraints>) -> Result<(), ProgramError> {
        instructions::cancel_event::handle_cancel_event(&mut ctx.accounts)
    }

    /// After a cancellation, a bettor reclaims their exact stake. The Bet
    /// account closes and leaves the bettor's User index.
    #[instruction(discriminator = 8)]
    pub fn claim_refund(ctx: Ctx<ClaimRefundAccountConstraints>) -> Result<(), ProgramError> {
        instructions::claim_refund::handle_claim_refund(&mut ctx.accounts)
    }

    /// Admin finalizes the outcome list and opens the market to bets. Needs at
    /// least two outcomes. Numbered after the existing handlers so their
    /// discriminators stay put.
    #[instruction(discriminator = 9)]
    pub fn open_betting(ctx: Ctx<OpenBettingAccountConstraints>) -> Result<(), ProgramError> {
        instructions::open_betting::handle_open_betting(&mut ctx.accounts)
    }
}
