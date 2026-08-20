use {
    crate::{
        instructions::shared::{err, error},
        state::Market,
        MarketAuthorityPda,
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
    /// Authority PDA over both vaults; holds no data, only signs.
    #[account(address = MarketAuthorityPda::seeds(market.address()))]
    pub market_authority: UncheckedAccount,
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

    let bump = [accounts.market.authority_bump];
    let market_address = *accounts.market.address();
    let seeds: &[Seed] = &[
        Seed::from(b"authority".as_ref()),
        Seed::from(market_address.as_ref()),
        Seed::from(&bump as &[u8]),
    ];

    if base_amount > 0 {
        accounts
            .token_program
            .transfer_checked(
                &accounts.base_vault,
                &accounts.base_mint,
                &accounts.operator_base,
                &accounts.market_authority,
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
                &accounts.market_authority,
                quote_amount,
                accounts.quote_mint.decimals(),
            )
            .invoke_signed(seeds)?;
    }

    Ok(())
}
