use anchor_lang::prelude::*;

declare_id!("2phbC62wekpw95XuBk4i1KX4uA8zBUWmYbiTMhicSuBV");

#[program]
pub mod hello_solana {
    use super::*;

    pub fn hello(_context: &mut Context<HelloAccountConstraints>) -> Result<()> {
        msg!("Hello, Solana!");

        msg!("Our program's Program ID: {}", &id());

        Ok(())
    }
}

#[derive(Accounts)]
pub struct HelloAccountConstraints {}
