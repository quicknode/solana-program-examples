use anchor_lang::prelude::*;

mod instructions;
use instructions::*;

declare_id!("4XCDGMD8fsdjUzmYj6d9if8twFt1f23Ym52iDmWK8fFs");

#[program]
pub mod group {

    use super::*;

    pub fn test_initialize_group(context: &mut Context<InitializeGroupAccountConstraints>) -> Result<()> {
        instructions::test_initialize_group::handler(context)
    }
}
