use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;

use crate::constants::LENDING_MARKET_SEED;
use crate::state::LendingMarket;

pub fn handle_initialize_lending_market(
    context: Context<InitializeLendingMarket>,
    market_id: u64,
) -> Result<()> {
    let market = &mut context.accounts.lending_market;
    market.market_id = market_id;
    market.owner = context.accounts.owner.key();
    market.quote_currency_mint = context.accounts.quote_currency_mint.key();
    market.bump = context.bumps.lending_market;
    Ok(())
}

#[derive(Accounts)]
#[instruction(market_id: u64)]
pub struct InitializeLendingMarket<'info> {
    // Seeded by `market_id` alone — the market is not identified by any
    // individual's address. `owner` is stored as a field and used only for
    // authorization (`has_one = owner`) on admin instructions.
    #[account(
        init,
        payer = owner,
        space = LendingMarket::DISCRIMINATOR.len() + LendingMarket::INIT_SPACE,
        seeds = [LENDING_MARKET_SEED, &market_id.to_le_bytes()],
        bump,
    )]
    pub lending_market: Account<'info, LendingMarket>,

    #[account(mut)]
    pub owner: Signer<'info>,

    pub quote_currency_mint: InterfaceAccount<'info, Mint>,

    pub system_program: Program<'info, System>,
}
