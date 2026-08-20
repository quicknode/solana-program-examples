use anchor_lang::prelude::*;

declare_id!("Eoiuq1dXvHxh6dLx3wh9gj8kSAUpga11krTrbfF5XYsC");

mod constants;
mod error;
mod instructions;
mod state;

pub use constants::*;
use error::*;
use instructions::*;

#[program]
pub mod fundraiser {
    use super::*;

    pub fn initialize_fundraiser(
        context: &mut Context<InitializeFundraiserAccountConstraints>,
        amount: u64,
        duration: u16,
    ) -> Result<()> {
        handle_initialize_fundraiser(&mut context.accounts, amount, duration, &context.bumps)?;

        Ok(())
    }

    pub fn contribute(
        context: &mut Context<ContributeAccountConstraints>,
        amount: u64,
    ) -> Result<()> {
        handle_contribute(&mut context.accounts, amount, &context.bumps)?;

        Ok(())
    }

    pub fn check_contributions(
        context: &mut Context<CheckContributionsAccountConstraints>,
    ) -> Result<()> {
        handle_check_contributions(&mut context.accounts)?;

        Ok(())
    }

    pub fn refund(context: &mut Context<RefundAccountConstraints>) -> Result<()> {
        handle_refund(&mut context.accounts)?;

        Ok(())
    }

    pub fn close_fundraiser(
        context: &mut Context<CloseFundraiserAccountConstraints>,
    ) -> Result<()> {
        handle_close_fundraiser(&mut context.accounts)?;

        Ok(())
    }
}
