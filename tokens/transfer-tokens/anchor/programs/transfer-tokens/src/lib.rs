use anchor_lang::prelude::*;

pub mod instructions;

use instructions::*;

declare_id!("nHi9DdNjuupjQ3c8AJU9sChB5gLbZvTLsJQouY4hU67");

#[program]
pub mod transfer_tokens {
    use super::*;

    pub fn create_token(
        context: &mut Context<CreateTokenAccountConstraints>,
        token_title: String,
        token_symbol: String,
        token_uri: String,
    ) -> Result<()> {
        create::handle_create_token(context, token_title, token_symbol, token_uri)
    }

    /// Mint `amount` minor units of the token to the recipient.
    pub fn mint_token(
        context: &mut Context<MintTokenAccountConstraints>,
        amount: u64,
    ) -> Result<()> {
        mint::handle_mint_token(context, amount)
    }

    /// Transfer `amount` minor units of the token from sender to recipient.
    pub fn transfer_tokens(
        context: &mut Context<TransferTokensAccountConstraints>,
        amount: u64,
    ) -> Result<()> {
        transfer::handle_transfer_tokens(context, amount)
    }
}
