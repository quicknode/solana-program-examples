#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

mod error;
pub mod instructions;
use instructions::*;
pub mod state;
#[cfg(test)]
mod tests;

declare_id!("qbuMdeYxYJXBjU6C6qFKjZKjXmrU83eDQomHdrch826");

/// Token escrow program: a maker deposits token A into a vault and specifies
/// how much of token B they want in return. A taker fulfils the offer by
/// sending the requested token B and receiving the deposited token A.
#[program]
mod quasar_escrow {
    use super::*;

    #[instruction(discriminator = 0)]
    pub fn make_offer(
        ctx: Ctx<MakeOfferAccountConstraints>,
        id: u64,
        deposit: u64,
        receive: u64,
    ) -> Result<(), ProgramError> {
        instructions::make_offer::handle_validate_offer(deposit, receive)?;
        instructions::make_offer::handle_make_offer(&mut ctx.accounts, id, receive, &ctx.bumps)?;
        instructions::make_offer::handle_deposit_tokens(&mut ctx.accounts, deposit)
    }

    /// The taker signs the terms they agreed to. An offer's address is its
    /// maker and id, so a maker can cancel and re-make the same id at worse
    /// terms while the taker's transaction is in flight; these bounds make
    /// that transaction fail instead of trading at the new terms.
    #[instruction(discriminator = 1)]
    pub fn take_offer(
        ctx: Ctx<TakeOfferAccountConstraints>,
        minimum_token_a_out: u64,
        maximum_token_b_in: u64,
    ) -> Result<(), ProgramError> {
        instructions::take_offer::handle_check_offer_terms(
            &ctx.accounts,
            minimum_token_a_out,
            maximum_token_b_in,
        )?;
        instructions::take_offer::handle_transfer_tokens(&mut ctx.accounts)?;
        instructions::take_offer::handle_withdraw_tokens_and_close_take(
            &mut ctx.accounts,
            &ctx.bumps,
        )
    }

    #[instruction(discriminator = 2)]
    pub fn cancel_offer(ctx: Ctx<CancelOfferAccountConstraints>) -> Result<(), ProgramError> {
        instructions::cancel_offer::handle_withdraw_tokens_and_close_cancel_offer(
            &mut ctx.accounts,
            &ctx.bumps,
        )
    }
}
