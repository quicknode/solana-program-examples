use anchor_lang::prelude::*;

mod instructions;
use instructions::*;

declare_id!("5BQyC7y2Pc283woThq11uZRqsgcRbBRLKz4yQ8BJadi2");

#[program]
pub mod memo_transfer {
    use super::*;

    pub fn initialize(context: Context<InitializeAccountConstraints>) -> Result<()> {
        instructions::initialize::handler(context)
    }

    pub fn disable(context: Context<DisableAccountConstraints>) -> Result<()> {
        instructions::disable::handler(context)
    }
}
