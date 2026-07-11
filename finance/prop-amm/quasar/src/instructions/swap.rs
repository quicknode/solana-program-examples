use {
    crate::{
        constants::{DIRECTION_BUY_BASE, DIRECTION_SELL_BASE},
        instructions::shared::{self, err, error},
        state::Market,
        MarketAuthorityPda,
    },
    quasar_lang::{prelude::*, sysvars::clock::Clock},
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct Swap {
    #[account(mut)]
    pub trader: Signer,
    #[account(
        address = Market::seeds(base_mint.address(), quote_mint.address()),
        has_one(oracle_feed),
        has_one(base_vault),
        has_one(quote_vault),
    )]
    pub market: Account<Market>,
    /// Authority PDA over both vaults; holds no data, only signs.
    #[account(address = MarketAuthorityPda::seeds(market.address()))]
    pub market_authority: UncheckedAccount,
    /// CHECK: bound to the market via `has_one(oracle_feed)`.
    pub oracle_feed: UncheckedAccount,
    pub base_mint: Account<Mint>,
    pub quote_mint: Account<Mint>,
    #[account(mut)]
    pub base_vault: Account<Token>,
    #[account(mut)]
    pub quote_vault: Account<Token>,
    /// The trader's base-token account. Unlike the Anchor sibling (which uses
    /// `init_if_needed`), it must already exist.
    #[account(mut)]
    pub trader_base: Account<Token>,
    #[account(mut)]
    pub trader_quote: Account<Token>,
    pub token_program: Program<TokenProgram>,
    pub clock: Sysvar<Clock>,
}

/// Fill a swap against the operator's quote. The price does not depend on the
/// vault balances, the size of the trade, or who traded before you: it is the
/// oracle price plus or minus the spread, full stop. The balances only decide
/// whether the market *can* fill you.
#[inline(always)]
pub fn handle_swap(
    accounts: &mut Swap,
    direction: u8,
    amount_in: u64,
    minimum_amount_out: u64,
) -> Result<(), ProgramError> {
    let market = &accounts.market;
    // Single-byte fields are plain in the zero-copy view; wider integers go
    // through `.get()`.
    if market.paused != 0 {
        return Err(err(error::MARKET_PAUSED));
    }
    if amount_in == 0 {
        return Err(err(error::ZERO_AMOUNT));
    }

    let oracle_scale = market.oracle_scale.get();
    let base_decimals = market.base_decimals;
    let quote_decimals = market.quote_decimals;
    let spread_bps = market.spread_bps.get();

    // Freshness, scale, and confidence are all enforced inside the read.
    let slot = accounts.clock.slot.get();
    let oracle_price = {
        let view = accounts.oracle_feed.to_account_view();
        let data = view
            .try_borrow()
            .map_err(|_| err(error::ORACLE_DATA_TOO_SHORT))?;
        shared::read_oracle_price(&data, oracle_scale, slot, market.max_confidence_bps.get())?
    };

    let (amount_out, respects_oracle_value) = match direction {
        DIRECTION_BUY_BASE => {
            let ask = shared::ask_price(oracle_price, spread_bps)?;
            let base_out = shared::base_out_for_quote_in(
                amount_in,
                ask,
                oracle_scale,
                base_decimals,
                quote_decimals,
            )?;
            let respects = shared::buy_respects_oracle_value(
                amount_in,
                base_out,
                oracle_price,
                oracle_scale,
                base_decimals,
                quote_decimals,
            )?;
            (base_out, respects)
        }
        DIRECTION_SELL_BASE => {
            let bid = shared::bid_price(oracle_price, spread_bps)?;
            let quote_out = shared::quote_out_for_base_in(
                amount_in,
                bid,
                oracle_scale,
                base_decimals,
                quote_decimals,
            )?;
            let respects = shared::sell_respects_oracle_value(
                amount_in,
                quote_out,
                oracle_price,
                oracle_scale,
                base_decimals,
                quote_decimals,
            )?;
            (quote_out, respects)
        }
        _ => return Err(err(error::INVALID_DIRECTION)),
    };

    if amount_out == 0 {
        return Err(err(error::AMOUNT_ROUNDS_TO_ZERO));
    }
    if amount_out < minimum_amount_out {
        return Err(err(error::SLIPPAGE_EXCEEDED));
    }

    // Assert after the math, not just before: the market must never hand out
    // more value than it took in, measured at the raw oracle price.
    if !respects_oracle_value {
        return Err(err(error::INVARIANT_VIOLATED));
    }

    let vault_out_balance = match direction {
        DIRECTION_BUY_BASE => accounts.base_vault.amount(),
        _ => accounts.quote_vault.amount(),
    };
    if amount_out > vault_out_balance {
        return Err(err(error::INSUFFICIENT_INVENTORY));
    }

    let bump = [accounts.market.authority_bump];
    let market_address = *accounts.market.address();
    let seeds: &[Seed] = &[
        Seed::from(b"authority".as_ref()),
        Seed::from(market_address.as_ref()),
        Seed::from(&bump as &[u8]),
    ];

    // The trader pays in, then the vault pays out, atomically or not at all.
    if direction == DIRECTION_BUY_BASE {
        accounts
            .token_program
            .transfer_checked(
                &accounts.trader_quote,
                &accounts.quote_mint,
                &accounts.quote_vault,
                &accounts.trader,
                amount_in,
                accounts.quote_mint.decimals(),
            )
            .invoke()?;
        accounts
            .token_program
            .transfer_checked(
                &accounts.base_vault,
                &accounts.base_mint,
                &accounts.trader_base,
                &accounts.market_authority,
                amount_out,
                accounts.base_mint.decimals(),
            )
            .invoke_signed(seeds)?;
    } else {
        accounts
            .token_program
            .transfer_checked(
                &accounts.trader_base,
                &accounts.base_mint,
                &accounts.base_vault,
                &accounts.trader,
                amount_in,
                accounts.base_mint.decimals(),
            )
            .invoke()?;
        accounts
            .token_program
            .transfer_checked(
                &accounts.quote_vault,
                &accounts.quote_mint,
                &accounts.trader_quote,
                &accounts.market_authority,
                amount_out,
                accounts.quote_mint.decimals(),
            )
            .invoke_signed(seeds)?;
    }

    Ok(())
}
