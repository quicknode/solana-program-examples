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
    context: Context<SwapAccountConstraints>,
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
            context.accounts.base_vault.amount,
            context.accounts.base_mint.decimals,
        ),
        Direction::SellBase => (
            context.accounts.quote_vault.amount,
            context.accounts.quote_mint.decimals,
        ),
    };
    require!(
        amount_out <= vault_out_balance,
        PropAmmError::InsufficientInventory
    );

    let market_key = market.key();
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
                    context.accounts.token_program.key(),
                    TransferChecked {
                        from: context.accounts.trader_quote.to_account_info(),
                        mint: context.accounts.quote_mint.to_account_info(),
                        to: context.accounts.quote_vault.to_account_info(),
                        authority: context.accounts.trader.to_account_info(),
                    },
                ),
                amount_in,
                context.accounts.quote_mint.decimals,
            )?;
            transfer_checked(
                CpiContext::new_with_signer(
                    context.accounts.token_program.key(),
                    TransferChecked {
                        from: context.accounts.base_vault.to_account_info(),
                        mint: context.accounts.base_mint.to_account_info(),
                        to: context.accounts.trader_base.to_account_info(),
                        authority: context.accounts.market_authority.to_account_info(),
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
                    context.accounts.token_program.key(),
                    TransferChecked {
                        from: context.accounts.trader_base.to_account_info(),
                        mint: context.accounts.base_mint.to_account_info(),
                        to: context.accounts.base_vault.to_account_info(),
                        authority: context.accounts.trader.to_account_info(),
                    },
                ),
                amount_in,
                context.accounts.base_mint.decimals,
            )?;
            transfer_checked(
                CpiContext::new_with_signer(
                    context.accounts.token_program.key(),
                    TransferChecked {
                        from: context.accounts.quote_vault.to_account_info(),
                        mint: context.accounts.quote_mint.to_account_info(),
                        to: context.accounts.trader_quote.to_account_info(),
                        authority: context.accounts.market_authority.to_account_info(),
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
pub struct SwapAccountConstraints<'info> {
    #[account(mut)]
    pub trader: Signer<'info>,

    #[account(
        seeds = [MARKET_SEED, market.base_mint.as_ref(), market.quote_mint.as_ref()],
        bump = market.bump,
        has_one = base_mint,
        has_one = quote_mint,
        has_one = oracle_feed,
        has_one = base_vault,
        has_one = quote_vault,
    )]
    pub market: Box<Account<'info, Market>>,

    /// CHECK: PDA authority over both vaults; holds no data, only signs.
    #[account(
        seeds = [AUTHORITY_SEED, market.key().as_ref()],
        bump = market.authority_bump,
    )]
    pub market_authority: UncheckedAccount<'info>,

    /// CHECK: validated by the `has_one = oracle_feed` constraint on the market.
    pub oracle_feed: UncheckedAccount<'info>,

    pub base_mint: Box<InterfaceAccount<'info, Mint>>,

    pub quote_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        mut,
        seeds = [BASE_VAULT_SEED, market.key().as_ref()],
        bump,
    )]
    pub base_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [QUOTE_VAULT_SEED, market.key().as_ref()],
        bump,
    )]
    pub quote_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = trader,
        associated_token::mint = base_mint,
        associated_token::authority = trader,
        associated_token::token_program = token_program,
    )]
    pub trader_base: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = trader,
        associated_token::mint = quote_mint,
        associated_token::authority = trader,
        associated_token::token_program = token_program,
    )]
    pub trader_quote: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}
