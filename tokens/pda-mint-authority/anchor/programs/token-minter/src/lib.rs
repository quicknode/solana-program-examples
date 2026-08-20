use anchor_lang::prelude::*;
use instructions::*;
pub mod instructions;

declare_id!("3LFrPHqwk5jMrmiz48BFj6NV2k4NjobgTe1jChzx3JGD");

#[program]
pub mod token_minter {
    use super::*;

    pub fn create_token(
        context: &mut Context<CreateTokenAccountConstraints>,
        token_name: String,
        token_symbol: String,
        token_uri: String,
    ) -> Result<()> {
        create::handle_create_token(context, token_name, token_symbol, token_uri)
    }

    /// Mint `amount` minor units of the token to the payer.
    pub fn mint_token(
        context: &mut Context<MintTokenAccountConstraints>,
        amount: u64,
    ) -> Result<()> {
        mint::handle_mint_token(context, amount)
    }
}
