#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

mod error;
pub mod instructions;
use instructions::*;
pub mod state;
#[cfg(test)]
mod tests;

declare_id!("Eoiuq1dXvHxh6dLx3wh9gj8kSAUpga11krTrbfF5XYsC");

/// Token crowdfunding program: a maker creates a fundraiser targeting a specific
/// SPL token. Contributors deposit tokens into a vault. If the target is met,
/// the maker withdraws everything. If not, contributors can reclaim their funds.
/// Once every contribution account is closed, the maker closes the fundraiser.
#[program]
mod quasar_fundraiser {
    use super::*;

    /// Create a new fundraiser with a target amount and duration.
    #[instruction(discriminator = 0)]
    pub fn initialize_fundraiser(
        ctx: Ctx<InitializeFundraiserAccountConstraints>,
        amount_to_raise: u64,
        duration: u16,
    ) -> Result<(), ProgramError> {
        instructions::handle_initialize_fundraiser(
            &mut ctx.accounts,
            amount_to_raise,
            duration,
            ctx.bumps.fundraiser,
        )
    }

    /// Contribute tokens to the fundraiser while its window is open. Creates
    /// the contributor's tracking account on first contribution.
    #[instruction(discriminator = 1)]
    pub fn contribute(
        ctx: Ctx<ContributeAccountConstraints>,
        amount: u64,
    ) -> Result<(), ProgramError> {
        instructions::handle_contribute(&mut ctx.accounts, amount, &ctx.bumps)
    }

    /// Maker withdraws all funds once the target is met, marking the
    /// fundraiser claimed. The fundraiser and the vault stay open until every
    /// contribution account is closed.
    #[instruction(discriminator = 2)]
    pub fn check_contributions(
        ctx: Ctx<CheckContributionsAccountConstraints>,
    ) -> Result<(), ProgramError> {
        instructions::handle_check_contributions(&mut ctx.accounts, &ctx.bumps)
    }

    /// Return a contributor's tokens after the deadline if the target was not
    /// met. Anyone may send it; the tokens and the rent go to the contributor.
    #[instruction(discriminator = 3)]
    pub fn refund(ctx: Ctx<RefundAccountConstraints>) -> Result<(), ProgramError> {
        instructions::handle_refund(&mut ctx.accounts, &ctx.bumps)
    }

    /// Close a contribution account once its fundraiser has been claimed,
    /// returning the rent to the contributor. Anyone may send it.
    #[instruction(discriminator = 4)]
    pub fn close_contribution(
        ctx: Ctx<CloseContributionAccountConstraints>,
    ) -> Result<(), ProgramError> {
        instructions::handle_close_contribution(&mut ctx.accounts)
    }

    /// Maker closes a finished fundraiser and its vault once no contributor
    /// account is open, so they can raise again at the same address.
    #[instruction(discriminator = 5)]
    pub fn close_fundraiser(
        ctx: Ctx<CloseFundraiserAccountConstraints>,
    ) -> Result<(), ProgramError> {
        instructions::handle_close_fundraiser(&mut ctx.accounts, &ctx.bumps)
    }
}
