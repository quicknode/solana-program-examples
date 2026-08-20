use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;

use crate::constants::LENDING_MARKET_SEED;
use crate::state::LendingMarket;

pub fn handle_initialize_lending_market(
    context: &mut Context<InitializeLendingMarket>,
    market_id: u64,
) -> Result<()> {
    let market = &mut context.accounts.lending_market;
    market.market_id = market_id;
    market.owner = *context.accounts.owner.address();
    market.quote_currency_mint = *context.accounts.quote_currency_mint.address();
    market.bump = context.bumps.lending_market;
    Ok(())
}

#[derive(Accounts)]
// The leading underscore is for rustc: `#[derive(Accounts)]` expands
// `_market_id` into a path that never reads it, so the plain name warns as
// unused. The `seeds` expression below is the real use.
#[instruction(_market_id: u64)]
pub struct InitializeLendingMarket {
    // Seeded by `market_id` alone — the market is not identified by any
    // individual's address. `owner` is stored as a field and used only for
    // authorization (`address = lending_market.owner`) on admin instructions.
    #[account(
        init,
        payer = owner,
        space = LendingMarket::DISCRIMINATOR.len() + LendingMarket::INIT_SPACE,
        seeds = [LENDING_MARKET_SEED, &_market_id.to_le_bytes()],
        bump,
    )]
    pub lending_market: BorshAccount<LendingMarket>,

    #[account(mut)]
    pub owner: Signer,

    pub quote_currency_mint: InterfaceAccount<Mint>,

    pub system_program: Program<System>,
}
