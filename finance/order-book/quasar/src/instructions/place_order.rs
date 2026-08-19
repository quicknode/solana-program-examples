use quasar_lang::cpi::Seed;
use quasar_lang::prelude::*;
use quasar_lang::remaining::RemainingAccounts;
// Anonymous import: brings the `Sysvar` trait's `Clock::get()` into scope
// without shadowing the `Sysvar<Rent>` account-wrapper type used below.
use quasar_lang::sysvars::Sysvar as _;
use quasar_spl::prelude::*;

use crate::errors::OrderBookError;
use crate::state::{
    add_open_order, load_order_book, load_order_book_mut, plan_fills, remove_open_order,
    snapshot_market_user, snapshot_order, Market, MarketUser, Order, OrderInner, OrderSide,
    OrderStatus, MARKET_SEED, MAX_OPEN_ORDERS,
};

// 10_000 bps == 100% - the universal rate convention on every major exchange.
const BASIS_POINTS_DENOMINATOR: u128 = 10_000;

// Remaining accounts arrive in groups of 2 per resting order we intend to
// cross: [maker_order, maker_market_user]. Fills land in the maker's
// unsettled_* balance (drained later via settle_funds), so the maker's ATAs
// aren't needed here - keeping the per-fill account footprint small, as in
// Openbook v2.
const ACCOUNTS_PER_MAKER: usize = 2;

/// raw token units for `lots × lot_size`, via a u128 intermediate so a
/// high-decimal mint can't overflow the multiply before it's range-checked.
fn raw_from_lots(lots: u64, lot_size: u64) -> Result<u64, ProgramError> {
    (lots as u128)
        .checked_mul(lot_size as u128)
        .ok_or(OrderBookError::NumericalOverflow)?
        .try_into()
        .map_err(|_| OrderBookError::NumericalOverflow.into())
}

/// raw quote units for `price × lots × quote_lot_size`.
fn quote_value(price: u64, lots: u64, quote_lot_size: u64) -> Result<u64, ProgramError> {
    (price as u128)
        .checked_mul(lots as u128)
        .ok_or(OrderBookError::NumericalOverflow)?
        .checked_mul(quote_lot_size as u128)
        .ok_or(OrderBookError::NumericalOverflow)?
        .try_into()
        .map_err(|_| OrderBookError::NumericalOverflow.into())
}

/// Taker fee on a fill's gross quote, rounded up (ceiling division) so the
/// protocol never leaks a minor unit to the maker across many tiny fills.
fn ceil_fee(gross_quote: u64, fee_basis_points: u16) -> Result<u64, ProgramError> {
    (gross_quote as u128)
        .checked_mul(fee_basis_points as u128)
        .ok_or(OrderBookError::NumericalOverflow)?
        .checked_add(BASIS_POINTS_DENOMINATOR - 1)
        .ok_or(OrderBookError::NumericalOverflow)?
        .checked_div(BASIS_POINTS_DENOMINATOR)
        .ok_or(OrderBookError::NumericalOverflow)?
        .try_into()
        .map_err(|_| OrderBookError::NumericalOverflow.into())
}

#[derive(Accounts)]
// Only `order_id` is referenced (for the Order PDA); the leading args must be
// listed so it lands in the right wire position, but are unused here.
#[instruction(_side: u8, _price: u64, _quantity: u64, order_id: u64)]
pub struct PlaceOrderAccountConstraints {
    // `has_one` ties every market-owned account to the addresses recorded on
    // the Market PDA. Without has_one on the vaults/mints a caller could swap
    // fee_vault in for quote_vault (same mint + authority) and steer the fee
    // transfer to drain real fees instead of routing them in.
    #[account(
        has_one(fee_vault) @ OrderBookError::InvalidFeeVault,
        has_one(base_vault) @ OrderBookError::InvalidBaseVault,
        has_one(quote_vault) @ OrderBookError::InvalidQuoteVault,
        has_one(base_mint) @ OrderBookError::InvalidBaseMint,
        has_one(quote_mint) @ OrderBookError::InvalidQuoteMint,
        has_one(order_book) @ OrderBookError::InvalidOrderBook,
    )]
    pub market: Account<Market>,

    // Zero-copy order book, accessed by casting its raw bytes (see
    // state/order_book.rs). Not a PDA - bound to `market` via has_one.
    #[account(mut)]
    pub order_book: UncheckedAccount,

    // The order id is supplied as an instruction argument so its PDA can be
    // derived here at parse time; the handler verifies it equals the book's
    // `next_order_id` before use, so it is not a free parameter.
    #[account(
        init,
        payer = owner,
        address = Order::seeds(market.address(), order_id)
    )]
    pub order: Account<Order>,

    #[account(mut, address = MarketUser::seeds(market.address(), owner.address()))]
    pub market_user: Account<MarketUser>,

    #[account(mut)]
    pub base_vault: Account<Token>,
    #[account(mut)]
    pub quote_vault: Account<Token>,
    #[account(mut)]
    pub fee_vault: Account<Token>,
    #[account(mut)]
    pub user_base_account: Account<Token>,
    #[account(mut)]
    pub user_quote_account: Account<Token>,

    pub base_mint: Account<Mint>,
    pub quote_mint: Account<Mint>,

    #[account(mut)]
    pub owner: Signer,

    pub rent: Sysvar<Rent>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
#[allow(clippy::too_many_arguments)]
pub fn handle_place_order(
    accounts: &mut PlaceOrderAccountConstraints,
    remaining: RemainingAccounts<'_>,
    side_byte: u8,
    price: u64,
    quantity: u64,
    order_id_arg: u64,
    bumps: &PlaceOrderAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    let side = OrderSide::from_u8(side_byte).ok_or(OrderBookError::InvalidSide)?;

    require!(
        accounts.market.is_active.is_true(),
        OrderBookError::MarketPaused
    );
    require!(price > 0, OrderBookError::InvalidPrice);

    let tick_size = u64::from(accounts.market.tick_size);
    require!(
        price.is_multiple_of(tick_size),
        OrderBookError::InvalidTickSize
    );

    let min_order_size = u64::from(accounts.market.min_order_size);
    require!(
        quantity >= min_order_size,
        OrderBookError::BelowMinOrderSize
    );

    require!(
        (accounts.market_user.open_orders_len as usize) < MAX_OPEN_ORDERS,
        OrderBookError::TooManyOpenOrders
    );

    let quote_lot_size = u64::from(accounts.market.quote_lot_size);
    let base_lot_size = u64::from(accounts.market.base_lot_size);
    let fee_basis_points = u16::from(accounts.market.fee_basis_points);
    let market_bump = accounts.market.bump;
    let base_mint_addr = accounts.market.base_mint;
    let quote_mint_addr = accounts.market.quote_mint;
    let market_key = *accounts.market.address();
    let owner_bytes = accounts.owner.address().to_bytes();

    // ---------------------------------------------------------------
    // Lock the funds the order would need if fully filled. Bids lock quote
    // (price × quantity × quote_lot_size); asks lock base (quantity ×
    // base_lot_size). Matching consumes from this locked pot; any unmatched
    // remainder rests with its lock in place.
    // ---------------------------------------------------------------
    match side {
        OrderSide::Bid => {
            let amount = quote_value(price, quantity, quote_lot_size)?;
            accounts
                .token_program
                .transfer_checked(
                    &accounts.user_quote_account,
                    &accounts.quote_mint,
                    &accounts.quote_vault,
                    &accounts.owner,
                    amount,
                    accounts.quote_mint.decimals,
                )
                .invoke()?;
        }
        OrderSide::Ask => {
            let amount = raw_from_lots(quantity, base_lot_size)?;
            accounts
                .token_program
                .transfer_checked(
                    &accounts.user_base_account,
                    &accounts.base_mint,
                    &accounts.base_vault,
                    &accounts.owner,
                    amount,
                    accounts.base_mint.decimals,
                )
                .invoke()?;
        }
    }

    // ---------------------------------------------------------------
    // Plan fills against the resting side, verifying the caller passed the
    // order id that matches the book's counter (so the Order PDA the client
    // derived is the one the book will assign).
    // ---------------------------------------------------------------
    let plan = {
        let view = accounts.order_book.to_account_view();
        // SAFETY: read-only cast of the order-book bytes; no other reference
        // to this account's data is live.
        let data = unsafe { core::slice::from_raw_parts(view.data_ptr(), view.data_len()) };
        let order_book = load_order_book(data)?;
        require!(
            order_book.next_order_id == order_id_arg,
            OrderBookError::OrderIdMismatch
        );
        plan_fills(order_book, side, price, quantity)
    };

    // ---------------------------------------------------------------
    // Apply fills: credit maker/taker balances, route the taker fee, and stamp
    // the maker Order accounts. Each fill's maker accounts arrive as a
    // remaining-account pair in price-time-priority order.
    // ---------------------------------------------------------------
    let mut taker_base_received: u64 = 0;
    let mut taker_quote_rebate: u64 = 0;
    let mut taker_quote_received: u64 = 0;
    // Aggregate per-fill fees into one transfer at the end - halves CU cost
    // vs one CPI per fill.
    let mut total_fee_quote: u64 = 0;

    for fill_index in 0..plan.count {
        let fill = plan.fills[fill_index];

        let mut order_ra = remaining
            .get(fill_index * ACCOUNTS_PER_MAKER)?
            .ok_or(OrderBookError::MissingMakerAccounts)?;
        let mut user_ra = remaining
            .get(fill_index * ACCOUNTS_PER_MAKER + 1)?
            .ok_or(OrderBookError::MissingMakerAccounts)?;

        // Validate owner (== this program) + discriminator, then take typed
        // mutable handles.
        let order_view = unsafe { order_ra.as_account_view_unchecked_mut() };
        Account::<Order>::from_account_view(&*order_view)?;
        let maker_order_acc =
            unsafe { Account::<Order>::from_account_view_unchecked_mut(order_view) };
        let mut maker_order = snapshot_order(maker_order_acc);

        let user_view = unsafe { user_ra.as_account_view_unchecked_mut() };
        Account::<MarketUser>::from_account_view(&*user_view)?;
        let maker_user_acc =
            unsafe { Account::<MarketUser>::from_account_view_unchecked_mut(user_view) };
        let mut maker_user = snapshot_market_user(maker_user_acc);

        require!(
            maker_order.order_id == fill.maker_order_id,
            OrderBookError::MakerAccountMismatch
        );
        require_keys_eq!(
            maker_order.market,
            market_key,
            OrderBookError::MakerAccountMismatch
        );
        require_keys_eq!(
            maker_order.owner,
            maker_user.owner,
            OrderBookError::MakerOwnerMismatch
        );
        require_keys_eq!(
            maker_user.market,
            market_key,
            OrderBookError::MakerAccountMismatch
        );

        // Fee model (maker-funded, no extra taker deposit):
        //   gross = fill_price × fill_quantity × quote_lot_size
        //   fee   = ceil(gross × fee_bps / 10_000)
        //   maker gets gross - fee, fee_vault gets fee, taker pays gross net
        //   out of their pre-locked quote.
        let gross_quote = quote_value(fill.fill_price, fill.fill_quantity, quote_lot_size)?;
        let fee_quote = ceil_fee(gross_quote, fee_basis_points)?;
        // Defensive: fees are a fraction of gross. `fee_bps <= 10_000` is
        // enforced at init, so this should be unreachable.
        require!(fee_quote <= gross_quote, OrderBookError::NumericalOverflow);

        match side {
            // Taker Bid, resting Ask. Taker pays quote, gets base.
            OrderSide::Bid => {
                let net_quote_to_maker = gross_quote
                    .checked_sub(fee_quote)
                    .ok_or(OrderBookError::NumericalOverflow)?;
                maker_user.unsettled_quote = maker_user
                    .unsettled_quote
                    .checked_add(net_quote_to_maker)
                    .ok_or(OrderBookError::NumericalOverflow)?;

                let base_from_fill = raw_from_lots(fill.fill_quantity, base_lot_size)?;
                taker_base_received = taker_base_received
                    .checked_add(base_from_fill)
                    .ok_or(OrderBookError::NumericalOverflow)?;

                // Price improvement: taker locked (price × qty) but only needs
                // (fill_price × qty) for this fill; refund the difference.
                let locked_for_this_fill = quote_value(price, fill.fill_quantity, quote_lot_size)?;
                let rebate = locked_for_this_fill
                    .checked_sub(gross_quote)
                    .ok_or(OrderBookError::NumericalOverflow)?;
                taker_quote_rebate = taker_quote_rebate
                    .checked_add(rebate)
                    .ok_or(OrderBookError::NumericalOverflow)?;
            }
            // Taker Ask, resting Bid. Taker gives base, gets quote.
            OrderSide::Ask => {
                let base_from_fill = raw_from_lots(fill.fill_quantity, base_lot_size)?;
                maker_user.unsettled_base = maker_user
                    .unsettled_base
                    .checked_add(base_from_fill)
                    .ok_or(OrderBookError::NumericalOverflow)?;

                let net_quote_to_taker = gross_quote
                    .checked_sub(fee_quote)
                    .ok_or(OrderBookError::NumericalOverflow)?;
                taker_quote_received = taker_quote_received
                    .checked_add(net_quote_to_taker)
                    .ok_or(OrderBookError::NumericalOverflow)?;
            }
        }

        total_fee_quote = total_fee_quote
            .checked_add(fee_quote)
            .ok_or(OrderBookError::NumericalOverflow)?;

        // Update the maker Order: bump filled_quantity, flip status.
        maker_order.filled_quantity = maker_order
            .filled_quantity
            .checked_add(fill.fill_quantity)
            .ok_or(OrderBookError::NumericalOverflow)?;
        let maker_fully_filled = maker_order.filled_quantity >= maker_order.original_quantity;
        maker_order.status = if maker_fully_filled {
            OrderStatus::Filled as u8
        } else {
            OrderStatus::PartiallyFilled as u8
        };
        if maker_fully_filled {
            remove_open_order(
                &mut maker_user.open_orders,
                &mut maker_user.open_orders_len,
                maker_order.order_id,
            );
        }

        maker_order_acc.set_inner(maker_order);
        maker_user_acc.set_inner(maker_user);
    }

    // ---------------------------------------------------------------
    // Apply the planned fills to the book (decrement remaining qty / remove
    // fully-filled leaves).
    // ---------------------------------------------------------------
    let maker_side = side.opposite();
    {
        let view = accounts.order_book.to_account_view();
        let data =
            unsafe { core::slice::from_raw_parts_mut(view.data_ptr() as *mut u8, view.data_len()) };
        let order_book = load_order_book_mut(data)?;
        for fill_index in 0..plan.count {
            let fill = plan.fills[fill_index];
            order_book.apply_fill_to_maker(
                maker_side,
                fill.maker_order_id,
                fill.fill_price,
                fill.fill_quantity,
            )?;
        }
    }

    // Move accumulated fee from quote_vault → fee_vault (one CPI signed by the
    // market PDA).
    if total_fee_quote > 0 {
        let bump = [market_bump];
        let seeds = [
            Seed::from(MARKET_SEED),
            Seed::from(base_mint_addr.as_ref()),
            Seed::from(quote_mint_addr.as_ref()),
            Seed::from(bump.as_ref()),
        ];
        accounts
            .token_program
            .transfer_checked(
                &accounts.quote_vault,
                &accounts.quote_mint,
                &accounts.fee_vault,
                &accounts.market,
                total_fee_quote,
                accounts.quote_mint.decimals,
            )
            .invoke_signed(&seeds)?;
    }

    // ---------------------------------------------------------------
    // Allocate the taker's order id (rolling the book counter) and rest any
    // unmatched remainder on the book.
    // ---------------------------------------------------------------
    let timestamp = i64::from(Clock::get()?.unix_timestamp);
    let order_id = {
        let view = accounts.order_book.to_account_view();
        let data =
            unsafe { core::slice::from_raw_parts_mut(view.data_ptr() as *mut u8, view.data_len()) };
        let order_book = load_order_book_mut(data)?;
        let id = order_book.allocate_order_id()?;
        if plan.taker_remaining > 0 {
            require!(
                !order_book.is_side_full(side),
                OrderBookError::OrderBookFull
            );
            order_book.place_resting(
                side,
                price,
                plan.taker_remaining,
                owner_bytes,
                id,
                timestamp,
            )?;
        }
        id
    };

    // Apply the taker's accumulated deltas + track the resting order.
    let mut taker_user = snapshot_market_user(&accounts.market_user);
    taker_user.unsettled_base = taker_user
        .unsettled_base
        .checked_add(taker_base_received)
        .ok_or(OrderBookError::NumericalOverflow)?;
    taker_user.unsettled_quote = taker_user
        .unsettled_quote
        .checked_add(taker_quote_rebate)
        .ok_or(OrderBookError::NumericalOverflow)?
        .checked_add(taker_quote_received)
        .ok_or(OrderBookError::NumericalOverflow)?;
    if plan.taker_remaining > 0 {
        add_open_order(
            &mut taker_user.open_orders,
            &mut taker_user.open_orders_len,
            order_id,
        );
    }
    accounts.market_user.set_inner(taker_user);

    // Stamp the taker's Order PDA. checked_sub, not saturating: a remainder
    // larger than the original would be a real matching-engine bug.
    let filled_quantity = quantity
        .checked_sub(plan.taker_remaining)
        .ok_or(OrderBookError::NumericalOverflow)?;
    let status = if plan.taker_remaining == 0 {
        OrderStatus::Filled
    } else if plan.taker_remaining < quantity {
        OrderStatus::PartiallyFilled
    } else {
        OrderStatus::Open
    };
    accounts.order.set_inner(OrderInner {
        market: market_key,
        owner: *accounts.owner.address(),
        order_id,
        side: side_byte,
        price,
        original_quantity: quantity,
        filled_quantity,
        status: status as u8,
        timestamp,
        bump: bumps.order,
    });

    Ok(())
}
