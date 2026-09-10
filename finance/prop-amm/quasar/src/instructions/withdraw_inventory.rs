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
pub struct WithdrawInventory {
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

/// The operator takes inventory back out — up to every token in both vaults,
/// at any time. There are no liquidity-provider shares because there are no
/// liquidity providers: the capital is the firm's own, so its exit needs no
/// waterfall, no share burn, and no pro-rata math.
#[inline(always)]
pub fn handle_withdraw_inventory(
    accounts: &mut WithdrawInventory,
    base_amount: u64,
    quote_amount: u64,
) -> Result<(), ProgramError> {
    if base_amount == 0 && quote_amount == 0 {
        return Err(err(error::ZERO_AMOUNT));
    }
    if base_amount > accounts.base_vault.amount() {
        return Err(err(error::INSUFFICIENT_INVENTORY));
    }
    if quote_amount > accounts.quote_vault.amount() {
        return Err(err(error::INSUFFICIENT_INVENTORY));
    }

    // The market owns both vaults and signs the withdrawal with its own seeds.
    let bump = [accounts.market.bump];
    let base_mint = *accounts.base_mint.address();
    let quote_mint = *accounts.quote_mint.address();
    let seeds: &[Seed] = &[
        Seed::from(b"market".as_ref()),
        Seed::from(base_mint.as_ref()),
        Seed::from(quote_mint.as_ref()),
        Seed::from(&bump as &[u8]),
    ];

    if base_amount > 0 {
        accounts
            .token_program
            .transfer_checked(
                &accounts.base_vault,
                &accounts.base_mint,
                &accounts.operator_base,
                &accounts.market,
                base_amount,
                accounts.base_mint.decimals(),
            )
            .invoke_signed(seeds)?;
    }

    if quote_amount > 0 {
        accounts
            .token_program
            .transfer_checked(
                &accounts.quote_vault,
                &accounts.quote_mint,
                &accounts.operator_quote,
                &accounts.market,
                quote_amount,
                accounts.quote_mint.decimals(),
            )
            .invoke_signed(seeds)?;
    }

    Ok(())
}
