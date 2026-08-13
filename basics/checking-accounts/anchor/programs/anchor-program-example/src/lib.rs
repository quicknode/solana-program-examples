use anchor_lang::prelude::*;

declare_id!("ECWPhR3rJbaPfyNFgphnjxSEexbTArc7vxD8fnW6tgKw");

#[program]
pub mod checking_account_program {
    use super::*;

    pub fn check_accounts(_context: &mut Context<CheckingAccountsAccountConstraints>) -> Result<()> {
        Ok(())
    }
}

// Account validation in Anchor is done using the types and constraints specified in the #[derive(Accounts)] structs
// This is a simple example and does not include all possible constraints and types
#[derive(Accounts)]
pub struct CheckingAccountsAccountConstraints {
    pub payer: Signer, // checks account is signer

    /// CHECK: No checks performed, example of an unchecked account
    #[account(mut)]
    pub account_to_create: UncheckedAccount,
    /// CHECK: Perform owner check using constraint
    #[account(
        mut,
        owner = id()
    )]
    pub account_to_change: UncheckedAccount,
    pub system_program: Program<System>, // checks account is executable, and is the system program
}
