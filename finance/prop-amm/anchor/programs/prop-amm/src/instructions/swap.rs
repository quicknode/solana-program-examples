use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked},
};

use crate::constants::{AUTHORITY_SEED, BASE_VAULT_SEED, MARKET_SEED, QUOTE_VAULT_SEED};
use crate::errors::PropAmmError;
use crate::quote_math;
use crate::state::oracle::read_oracle_price;
use crate::state::{Direction, Market};

/// Fill a swap against the operator's quote.
///
/// The price does not depend on the vault balances, on the size of the trade,
/// or on who traded before you: it is the oracle price plus or minus the
/// spread, full stop. The balances only decide whether the market *can* fill
/// you — a curve AMM's reserves are its pricing input, a prop AMM's inventory
/// is just its ammunition.
pub fn handle_swap(
    context: &mut Context<SwapAccountConstraints>,
    direction: Direction,
    amount_in: u64,
    minimum_amount_out: u64,
) -> Result<()> {
    let market = &context.accounts.market;
    require!(!market.paused, PropAmmError::MarketPaused);
    require!(amount_in > 0, PropAmmError::ZeroAmount);

    // Freshness, scale, and confidence are all enforced inside the read. For a
    // market maker the staleness bound is the business itself: a quote priced
    // off an old number is a free option for whoever notices first.
    let oracle_price = read_oracle_price(
        &context.accounts.oracle_feed,
        market.oracle_scale,
        market.max_confidence_bps,
    )?;

    let (amount_out, respects_oracle_value) = match direction {
        Direction::BuyBase => {
            let ask = quote_math::ask_price(oracle_price, market.spread_bps)
                .ok_or(PropAmmError::MathOverflow)?;
            let base_out = quote_math::base_out_for_quote_in(
                amount_in,
                ask,
                market.oracle_scale,
                market.base_decimals,
                market.quote_decimals,
            )
            .ok_or(PropAmmError::MathOverflow)?;
            let respects = quote_math::buy_respects_oracle_value(
                amount_in,
                base_out,
                oracle_price,
                market.oracle_scale,
                market.base_decimals,
                market.quote_decimals,
            )
            .ok_or(PropAmmError::MathOverflow)?;
            (base_out, respects)
        }
        Direction::SellBase => {
            let bid = quote_math::bid_price(oracle_price, market.spread_bps)
                .ok_or(PropAmmError::MathOverflow)?;
            let quote_out = quote_math::quote_out_for_base_in(
                amount_in,
                bid,
                market.oracle_scale,
                market.base_decimals,
                market.quote_decimals,
            )
            .ok_or(PropAmmError::MathOverflow)?;
            let respects = quote_math::sell_respects_oracle_value(
                amount_in,
                quote_out,
                oracle_price,
                market.oracle_scale,
                market.base_decimals,
                market.quote_decimals,
            )
            .ok_or(PropAmmError::MathOverflow)?;
            (quote_out, respects)
        }
    };

    require!(amount_out > 0, PropAmmError::AmountRoundsToZero);
    require!(
        amount_out >= minimum_amount_out,
        PropAmmError::SlippageExceeded
    );

    // Assert after the math, not just before: whatever the quoting arithmetic
    // above produced, the market must never hand out more value than it took
    // in, measured at the raw oracle price. The spread and the rounding
    // directions make this true by construction; this check makes it true even
    // if a refactor breaks one of them.
    require!(respects_oracle_value, PropAmmError::InvariantViolated);

    let (vault_out_balance, out_mint_decimals) = match direction {
        Direction::BuyBase => (
            context.accounts.base_vault.amount(),
            context.accounts.base_mint.decimals(),
        ),
        Direction::SellBase => (
            context.accounts.quote_vault.amount(),
            context.accounts.quote_mint.decimals(),
        ),
    };
    require!(
        amount_out <= vault_out_balance,
        PropAmmError::InsufficientInventory
    );

    let market_key = market.address();
    let authority_seeds: &[&[u8]] = &[
        AUTHORITY_SEED,
        market_key.as_ref(),
        &[market.authority_bump],
    ];

    // The trader pays in, then the vault pays out, atomically or not at all.
    match direction {
        Direction::BuyBase => {
            transfer_checked(
                CpiContext::new(
                    context.accounts.token_program.address(),
                    TransferChecked {
                        from: context.accounts.trader_quote.to_cpi_handle_mut(),
                        mint: context.accounts.quote_mint.to_cpi_handle(),
                        to: context.accounts.quote_vault.to_cpi_handle_mut(),
                        authority: context.accounts.trader.cpi_handle(),
                    },
                ),
                amount_in,
                context.accounts.quote_mint.decimals(),
            )?;
            transfer_checked(
                CpiContext::new_with_signer(
                    context.accounts.token_program.address(),
                    TransferChecked {
                        from: context.accounts.base_vault.to_cpi_handle_mut(),
                        mint: context.accounts.base_mint.to_cpi_handle(),
                        to: context.accounts.trader_base.to_cpi_handle_mut(),
                        authority: context.accounts.market_authority.cpi_handle(),
                    },
                    &[authority_seeds],
                ),
                amount_out,
                out_mint_decimals,
            )?;
        }
        Direction::SellBase => {
            transfer_checked(
                CpiContext::new(
                    context.accounts.token_program.address(),
                    TransferChecked {
                        from: context.accounts.trader_base.to_cpi_handle_mut(),
                        mint: context.accounts.base_mint.to_cpi_handle(),
                        to: context.accounts.base_vault.to_cpi_handle_mut(),
                        authority: context.accounts.trader.cpi_handle(),
                    },
                ),
                amount_in,
                context.accounts.base_mint.decimals(),
            )?;
            transfer_checked(
                CpiContext::new_with_signer(
                    context.accounts.token_program.address(),
                    TransferChecked {
                        from: context.accounts.quote_vault.to_cpi_handle_mut(),
                        mint: context.accounts.quote_mint.to_cpi_handle(),
                        to: context.accounts.trader_quote.to_cpi_handle_mut(),
                        authority: context.accounts.market_authority.cpi_handle(),
                    },
                    &[authority_seeds],
                ),
                amount_out,
                out_mint_decimals,
            )?;
        }
    }

    Ok(())
}

#[derive(Accounts)]
pub struct SwapAccountConstraints {
    #[account(mut)]
    pub trader: Signer,

    #[account(
        seeds = [MARKET_SEED, market.base_mint.as_ref(), market.quote_mint.as_ref()],
        bump = market.bump,
    )]
    pub market: Box<BorshAccount<Market>>,

    /// CHECK: PDA authority over both vaults; holds no data, only signs.
    #[account(
        seeds = [AUTHORITY_SEED, market.address().as_ref()],
        bump = market.authority_bump,
    )]
    pub market_authority: UncheckedAccount,

    /// CHECK: validated by the `address = market.oracle_feed` constraint below.
    #[account(address = market.oracle_feed)]
    pub oracle_feed: UncheckedAccount,

    #[account(address = market.base_mint)]
    pub base_mint: Box<InterfaceAccount<Mint>>,

    #[account(address = market.quote_mint)]
    pub quote_mint: Box<InterfaceAccount<Mint>>,

    #[account(
        mut,
        seeds = [BASE_VAULT_SEED, market.address().as_ref()],
        bump,
        address = market.base_vault,
    )]
    pub base_vault: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        seeds = [QUOTE_VAULT_SEED, market.address().as_ref()],
        bump,
        address = market.quote_vault,
    )]
    pub quote_vault: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        init_if_needed,
        payer = trader,
        associated_token::mint = base_mint,
        associated_token::authority = trader,
        associated_token::token_program = token_program,
    )]
    pub trader_base: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        init_if_needed,
        payer = trader,
        associated_token::mint = quote_mint,
        associated_token::authority = trader,
        associated_token::token_program = token_program,
    )]
    pub trader_quote: Box<InterfaceAccount<TokenAccount>>,

    pub token_program: Interface<'static, TokenInterface>,
    pub associated_token_program: Program<AssociatedToken>,
    pub system_program: Program<System>,
}
