#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

pub mod errors;
pub mod instructions;
pub mod state;

use instructions::*;

#[cfg(test)]
mod tests;

declare_id!("C69UJ8irfmHq5ysyLek7FKApHR86FBeupiz4JnoyPzzx");

/// Central limit order book (CLOB) for a single (base, quote) token pair. Users
/// post bids or asks at their chosen prices; the program crosses opposing
/// orders in price-time priority, credits fills to maker/taker unsettled
/// balances, routes the taker fee to a fee vault, and rests any unmatched
/// remainder on the book. See README.md for the full walkthrough.
#[program]
mod quasar_order_book {
    use super::*;

    /// Create a market for a (base, quote) pair. The Market PDA, the two
    /// PDA-authority vaults, and the fee vault are created here; the client
    /// pre-creates the large order-book account (see `initialize_market`).
    #[instruction(discriminator = 0)]
    pub fn initialize_market(
        ctx: Ctx<InitializeMarketAccountConstraints>,
        fee_basis_points: u16,
        tick_size: u64,
        base_lot_size: u64,
        quote_lot_size: u64,
        min_order_size: u64,
    ) -> Result<(), ProgramError> {
        instructions::initialize_market::handle_initialize_market(
            &mut ctx.accounts,
            fee_basis_points,
            tick_size,
            base_lot_size,
            quote_lot_size,
            min_order_size,
            &ctx.bumps,
        )
    }

    /// Create a per-user, per-market account tracking a user's open orders and
    /// unsettled balances.
    #[instruction(discriminator = 1)]
    pub fn initialize_market_user(
        ctx: Ctx<InitializeMarketUserAccountConstraints>,
    ) -> Result<(), ProgramError> {
        instructions::initialize_market_user::handle_initialize_market_user(
            &mut ctx.accounts,
            &ctx.bumps,
        )
    }

    /// Place a bid or ask (`side`: 0 = Bid, 1 = Ask). Locks the required funds,
    /// crosses the opposing side of the book in price-time priority, credits
    /// fills to maker/taker `unsettled_*` balances, routes the taker fee to the
    /// fee vault, and rests any remainder at the caller's limit price.
    ///
    /// Resting maker orders to cross are supplied as remaining accounts, in
    /// pairs of `(maker_order, maker_market_user)`, in the book's price-time
    /// priority. `order_id` must equal the book's current `next_order_id`.
    #[instruction(discriminator = 2)]
    pub fn place_order(
        ctx: CtxWithRemaining<PlaceOrderAccountConstraints>,
        side: u8,
        price: u64,
        quantity: u64,
        order_id: u64,
    ) -> Result<(), ProgramError> {
        let remaining = ctx.remaining_accounts();
        instructions::place_order::handle_place_order(
            &mut ctx.accounts,
            remaining,
            side,
            price,
            quantity,
            order_id,
            &ctx.bumps,
        )
    }

    /// Cancel an open (or partially filled) order. Credits the remaining locked
    /// amount back to the owner's unsettled balance; the token transfer happens
    /// on settle_funds.
    #[instruction(discriminator = 3)]
    pub fn cancel_order(ctx: Ctx<CancelOrderAccountConstraints>) -> Result<(), ProgramError> {
        instructions::cancel_order::handle_cancel_order(&mut ctx.accounts)
    }

    /// Move accumulated unsettled balances out of the market vaults into the
    /// user's token accounts. No-op if both balances are zero.
    #[instruction(discriminator = 4)]
    pub fn settle_funds(ctx: Ctx<SettleFundsAccountConstraints>) -> Result<(), ProgramError> {
        instructions::settle_funds::handle_settle_funds(&mut ctx.accounts)
    }

    /// Drain the fee vault into the market authority's token account.
    /// Authority-gated - only the market's stored `authority` may call this.
    #[instruction(discriminator = 5)]
    pub fn withdraw_fees(ctx: Ctx<WithdrawFeesAccountConstraints>) -> Result<(), ProgramError> {
        instructions::withdraw_fees::handle_withdraw_fees(&mut ctx.accounts)
    }
}
