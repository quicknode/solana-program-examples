#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

mod instructions;
use instructions::*;
mod state;
#[cfg(test)]
mod tests;

declare_id!("22222222222222222222222222222222222222222222");

/// Token escrow program: a maker deposits token A into a vault and specifies
/// how much of token B they want in return. A taker fulfils the offer by
/// sending the requested token B and receiving the deposited token A.
#[program]
mod quasar_escrow {
    use super::*;

    #[instruction(discriminator = 0)]
    pub fn make_offer(ctx: Ctx<MakeOffer>, deposit: u64, receive: u64) -> Result<(), ProgramError> {
        instructions::make_offer::handle_make_offer(&mut ctx.accounts, receive, &ctx.bumps)?;
        instructions::make_offer::handle_deposit_tokens(&mut ctx.accounts, deposit)
    }

    #[instruction(discriminator = 1)]
    pub fn take_offer(ctx: Ctx<TakeOffer>) -> Result<(), ProgramError> {
        instructions::take_offer::handle_transfer_tokens(&mut ctx.accounts)?;
        instructions::take_offer::handle_withdraw_tokens_and_close_take(&mut ctx.accounts, &ctx.bumps)
    }

    #[instruction(discriminator = 2)]
    pub fn cancel_offer(ctx: Ctx<CancelOffer>) -> Result<(), ProgramError> {
        instructions::cancel_offer::handle_withdraw_tokens_and_close_cancel_offer(&mut ctx.accounts, &ctx.bumps)
    }
}
