use anchor_lang::prelude::*;

mod instructions;
use instructions::*;

declare_id!("A9rxKS84ZoJVyeTfQbCEfxME2vvAM4uwSMjkmhR5XWb1");

#[program]
pub mod permanent_delegate {
    use super::*;

    pub fn initialize(context: Context<InitializeAccountConstraints>) -> Result<()> {
        instructions::initialize::handler(context)
    }
}
