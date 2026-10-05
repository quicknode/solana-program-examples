use anchor_lang::prelude::*;

pub mod errors;
pub mod instructions;
pub mod state;

use instructions::*;

declare_id!("C69UJ8irfmHq5ysyLek7FKApHR86FBeupiz4JnoyPzzx");

#[program]
pub mod order_book {
    use super::*;

    /// Create a new market for a (base, quote) pair. Deploys the market PDA,
    /// the order book PDA, and the two PDA-authority vaults that hold locked
    /// funds while orders are open.
    pub fn initialize_market(
        context: Context<InitializeMarketAccountConstraints>,
        fee_basis_points: u16,
        tick_size: u64,
        base_lot_size: u64,
        quote_lot_size: u64,
        min_order_size: u64,
    ) -> Result<()> {
        instructions::initialize_market::handle_initialize_market(
            context,
            fee_basis_points,
            tick_size,
            base_lot_size,
            quote_lot_size,
            min_order_size,
        )
    }

    /// Create a per-user, per-market account that tracks a user's open orders
    /// and unsettled balances.
    pub fn initialize_market_user(
        context: Context<InitializeMarketUserAccountConstraints>,
    ) -> Result<()> {
        instructions::initialize_market_user::handle_initialize_market_user(context)
    }

    /// Place a bid or ask. Locks the required funds (quote for bids, base
    /// for asks) into the market vault, crosses against the opposing side
    /// of the book using price-time priority (best price first, earliest
    /// timestamp at a tie), credits fills to maker/taker `unsettled_*`
    /// balances, routes the taker fee to the fee vault, and rests any
    /// unmatched remainder on the book at the caller's limit price.
    ///
    /// Callers supply resting orders to cross against as
    /// `remaining_accounts`, in pairs of
    /// `(maker_order_pda, maker_user_account_pda)`, ordered by the
    /// book's price-time priority (i.e. best ask first for a taker bid).
    pub fn place_order<'info>(
        context: Context<'info, PlaceOrderAccountConstraints<'info>>,
        side: state::OrderSide,
        price: u64,
        quantity: u64,
    ) -> Result<()> {
        instructions::place_order::handle_place_order(context, side, price, quantity)
    }

    /// Cancel an open (or partially filled) order. Credits the remaining
    /// locked amount back to the owner's unsettled balance; the actual token
    /// transfer happens on settle_funds.
    pub fn cancel_order(context: Context<CancelOrderAccountConstraints>) -> Result<()> {
        instructions::cancel_order::handle_cancel_order(context)
    }

    /// Move accumulated unsettled balances out of the market vault and into
    /// the user's token accounts. No-op if both balances are zero.
    pub fn settle_funds(context: Context<SettleFundsAccountConstraints>) -> Result<()> {
        instructions::settle_funds::handle_settle_funds(context)
    }

    /// Close a Filled or Cancelled order's account and return its rent to
    /// the order's owner, who signs. An order still resting on the book
    /// (Open or PartiallyFilled) is refused with `OrderNotClosable`.
    pub fn close_order(context: Context<CloseOrderAccountConstraints>) -> Result<()> {
        instructions::close_order::handle_close_order(context)
    }

    /// Close the owner's MarketUser account for this market and return its
    /// rent to them. Refused with `MarketUserNotClosable` while the account
    /// lists an open order or holds an unsettled balance.
    pub fn close_market_user(context: Context<CloseMarketUserAccountConstraints>) -> Result<()> {
        instructions::close_market_user::handle_close_market_user(context)
    }

    /// Drain the fee vault into the market authority's token account.
    /// Authority-gated - only the market's stored `authority` may call this.
    pub fn withdraw_fees(context: Context<WithdrawFeesAccountConstraints>) -> Result<()> {
        instructions::withdraw_fees::handle_withdraw_fees(context)
    }

    /// Stop the market taking new orders: `place_order` is refused with
    /// `MarketPaused` until `resume_market`. Cancels, settlements and fee
    /// withdrawals keep working while the market is paused. Only the
    /// market's stored `authority` may call this.
    pub fn pause_market(context: Context<PauseMarketAccountConstraints>) -> Result<()> {
        instructions::pause_market::handle_pause_market(context)
    }

    /// Reopen a paused market so `place_order` accepts orders again. Only
    /// the market's stored `authority` may call this.
    pub fn resume_market(context: Context<ResumeMarketAccountConstraints>) -> Result<()> {
        instructions::resume_market::handle_resume_market(context)
    }
}
