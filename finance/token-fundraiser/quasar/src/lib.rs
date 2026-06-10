#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

mod error;
mod instructions;
use instructions::*;
mod state;
#[cfg(test)]
mod tests;

declare_id!("Eoiuq1dXvHxh6dLx3wh9gj8kSAUpga11krTrbfF5XYsC");

/// Token crowdfunding program: a maker creates a fundraiser targeting a specific
/// SPL token. Contributors deposit tokens into a vault. If the target is met,
/// the maker withdraws everything. If not, contributors can reclaim their funds.
#[program]
mod quasar_token_fundraiser {
    use super::*;

    /// Create a new fundraiser with a target amount and duration.
    #[instruction(discriminator = 0)]
    pub fn initialize(
        ctx: Ctx<Initialize>,
        amount_to_raise: u64,
        duration: u16,
    ) -> Result<(), ProgramError> {
        instructions::handle_initialize(&mut ctx.accounts, amount_to_raise, duration, ctx.bumps.fundraiser)
    }

    /// Contribute tokens to the fundraiser while its window is open. Creates
    /// the contributor's tracking account on first contribution.
    #[instruction(discriminator = 1)]
    pub fn contribute(ctx: Ctx<Contribute>, amount: u64) -> Result<(), ProgramError> {
        instructions::handle_contribute(&mut ctx.accounts, amount, &ctx.bumps)
    }

    /// Maker withdraws all funds once the target is met.
    #[instruction(discriminator = 2)]
    pub fn check_contributions(ctx: Ctx<CheckContributions>) -> Result<(), ProgramError> {
        instructions::handle_check_contributions(&mut ctx.accounts, &ctx.bumps)
    }

    /// Contributors reclaim their tokens after the deadline if the target
    /// was not met.
    #[instruction(discriminator = 3)]
    pub fn refund(ctx: Ctx<Refund>) -> Result<(), ProgramError> {
        instructions::handle_refund(&mut ctx.accounts, &ctx.bumps)
    }
}
