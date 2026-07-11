use {
    crate::{
        instructions::shared::{err, error},
        state::Market,
    },
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct DepositInventory {
    // `has_one(operator)` on the market is the whole access control: only the
    // firm's key can stock the market. The vaults are bound to the market by
    // `has_one` too, matching the addresses stored at creation.
    #[account(mut)]
    pub operator: Signer,
    #[account(
        address = Market::seeds(base_mint.address(), quote_mint.address()),
        has_one(operator),
        has_one(base_vault),
        has_one(quote_vault),
    )]
    pub market: Account<Market>,
    pub base_mint: Account<Mint>,
    pub quote_mint: Account<Mint>,
    #[account(mut)]
    pub base_vault: Account<Token>,
    #[account(mut)]
    pub quote_vault: Account<Token>,
    #[account(mut)]
    pub operator_base: Account<Token>,
    #[account(mut)]
    pub operator_quote: Account<Token>,
    pub token_program: Program<TokenProgram>,
}

#[inline(always)]
pub fn handle_deposit_inventory(
    accounts: &mut DepositInventory,
    base_amount: u64,
    quote_amount: u64,
) -> Result<(), ProgramError> {
    if base_amount == 0 && quote_amount == 0 {
        return Err(err(error::ZERO_AMOUNT));
    }

    if base_amount > 0 {
        accounts
            .token_program
            .transfer_checked(
                &accounts.operator_base,
                &accounts.base_mint,
                &accounts.base_vault,
                &accounts.operator,
                base_amount,
                accounts.base_mint.decimals(),
            )
            .invoke()?;
    }

    if quote_amount > 0 {
        accounts
            .token_program
            .transfer_checked(
                &accounts.operator_quote,
                &accounts.quote_mint,
                &accounts.quote_vault,
                &accounts.operator,
                quote_amount,
                accounts.quote_mint.decimals(),
            )
            .invoke()?;
    }

    Ok(())
}
