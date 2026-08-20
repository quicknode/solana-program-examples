use anchor_lang::prelude::*;

mod instructions;
use instructions::*;

declare_id!("AcfQLsYKuzprcCNH1n96pKKgAbAnZchwpbr3gbVN742n");

#[program]
pub mod mint_close_authority {
    use super::*;

    pub fn initialize(context: &mut Context<InitializeAccountConstraints>) -> Result<()> {
        instructions::initialize::handler(context)
    }

    pub fn close(context: &mut Context<CloseAccountConstraints>) -> Result<()> {
        instructions::close::handler(context)
    }
}
