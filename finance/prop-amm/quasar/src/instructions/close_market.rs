use {
    crate::{
        instructions::shared::{err, error},
        state::Market,
    },
    quasar_lang::cpi::Seed,
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct CloseMarket {
    #[account(mut)]
    pub operator: Signer,
    #[account(
        mut,
        address = Market::seeds(base_mint.address(), quote_mint.address()),
        has_one(operator),
        has_one(base_vault),
        has_one(quote_vault),
        close(dest = operator),
    )]
    pub market: Account<Market>,
    pub base_mint: Account<Mint>,
    pub quote_mint: Account<Mint>,
    #[account(mut)]
    pub base_vault: Account<Token>,
    #[account(mut)]
    pub quote_vault: Account<Token>,
    pub token_program: Program<TokenProgram>,
}

/// The operator shuts the market down and takes back the rent it paid for the
/// market account and its two vaults.
///
/// Only the operator can close, through the same `has_one(operator)`
/// constraint that guards `withdraw_inventory`. Both vaults must be empty: the
/// tokens in them are the operator's inventory, and closing a token account
/// that still holds tokens fails, so the handler refuses with
/// `INVENTORY_NOT_EMPTY` and the operator withdraws first. That includes any
/// tokens someone transferred straight into a vault; they are inventory like
/// the rest, and `withdraw_inventory` pays them out. The market account then
/// closes through its `close(dest = operator)` constraint, so all three rents
/// go back to the operator.
#[inline(always)]
pub fn handle_close_market(accounts: &mut CloseMarket) -> Result<(), ProgramError> {
    if accounts.base_vault.amount() != 0 || accounts.quote_vault.amount() != 0 {
        return Err(err(error::INVENTORY_NOT_EMPTY));
    }

    // The market owns both vaults and signs their closure with its own seeds.
    let bump = [accounts.market.bump];
    let base_mint = *accounts.base_mint.address();
    let quote_mint = *accounts.quote_mint.address();
    let seeds: &[Seed] = &[
        Seed::from(b"market".as_ref()),
        Seed::from(base_mint.as_ref()),
        Seed::from(quote_mint.as_ref()),
        Seed::from(&bump as &[u8]),
    ];

    accounts
        .token_program
        .close_account(&accounts.base_vault, &accounts.operator, &accounts.market)
        .invoke_signed(seeds)?;
    accounts
        .token_program
        .close_account(&accounts.quote_vault, &accounts.operator, &accounts.market)
        .invoke_signed(seeds)?;

    Ok(())
}
