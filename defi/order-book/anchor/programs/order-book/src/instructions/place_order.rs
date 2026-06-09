use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::errors::ErrorCode;
use crate::state::{
    add_open_order, plan_fills, remove_open_order, Market, Order, OrderBook, OrderSide,
    OrderStatus, MarketUser, MARKET_SEED, ORDER_SEED, MARKET_USER_SEED,
};

// Mirror of MarketUser.open_orders max_len. Kept as a constant so the
// PlaceOrder check reads clearly and the limit is documented in one place.
const MAX_OPEN_ORDERS_PER_USER: usize = 20;

// Basis-points denominator. 10_000 bps == 100% — the universal rate convention
// on every major exchange (NYSE, CME, Binance, Coinbase, ...).
const BASIS_POINTS_DENOMINATOR: u128 = 10_000;

// Remaining accounts are passed in groups of 2 per resting order we intend
// to cross: [maker_order, maker_market_user]. We keep it at 2 (instead of
// also threading the maker's ATAs) because fills land in the maker's
// unsettled_* balance — the maker drains them later via settle_funds. This
// mirrors how Openbook v2 works and keeps the per-fill account footprint
// small.
const ACCOUNTS_PER_MAKER: usize = 2;

pub fn handle_place_order<'info>(
    context: Context<'info, PlaceOrder<'info>>,
    side: OrderSide,
    price: u64,
    quantity: u64,
) -> Result<()> {
    let market = &context.accounts.market;

    require!(market.is_active, ErrorCode::MarketPaused);
    require!(price > 0, ErrorCode::InvalidPrice);
    require!(price % market.tick_size == 0, ErrorCode::InvalidTickSize);
    require!(
        quantity >= market.min_order_size,
        ErrorCode::BelowMinOrderSize
    );

    require!(
        context.accounts.market_user.open_orders.len() < MAX_OPEN_ORDERS_PER_USER,
        ErrorCode::TooManyOpenOrders
    );

    // Lock up the funds the order would need if filled. Bids lock quote
    // (price * quantity); asks lock base (quantity). This always happens —
    // matching consumes from the locked pot (already in the vault), and any
    // unmatched remainder rests as a maker order with its lock still in place.
    //
    // The bid lock multiplies two u64s. A plain `u64::checked_mul` would
    // refuse anything that overflows u64 (~1.8e19) — which is a perfectly
    // legal lock once you scale by token decimals (e.g. 18-decimal quote
    // mint * mid-cap price * mid-cap quantity). Promote to u128 for the
    // multiplication, then narrow back to u64 with try_into so the failure
    // mode is "can't fit the onchain transfer" not "silently rejected at
    // the math step".
    let (source_account, mint_account_info, decimals, transfer_amount, destination_vault) =
        match side {
            OrderSide::Bid => (
                context.accounts.user_quote_account.to_account_info(),
                context.accounts.quote_mint.to_account_info(),
                context.accounts.quote_mint.decimals,
                (price as u128)
                    .checked_mul(quantity as u128)
                    .ok_or(ErrorCode::NumericalOverflow)?
                    .try_into()
                    .map_err(|_| error!(ErrorCode::NumericalOverflow))?,
                context.accounts.quote_vault.to_account_info(),
            ),
            OrderSide::Ask => (
                context.accounts.user_base_account.to_account_info(),
                context.accounts.base_mint.to_account_info(),
                context.accounts.base_mint.decimals,
                (quantity as u128)
                    .checked_mul(market.base_lot_size as u128)
                    .ok_or(ErrorCode::NumericalOverflow)?
                    .try_into()
                    .map_err(|_| error!(ErrorCode::NumericalOverflow))?,
                context.accounts.base_vault.to_account_info(),
            ),
        };

    transfer_checked(
        CpiContext::new(
            context.accounts.token_program.key(),
            TransferChecked {
                from: source_account,
                mint: mint_account_info,
                to: destination_vault,
                authority: context.accounts.owner.to_account_info(),
            },
        ),
        transfer_amount,
        decimals,
    )?;

    // ---------------------------------------------------------------
    // Matching plan
    // ---------------------------------------------------------------
    //
    // Caller passes pairs of `(maker_order, maker_market_user)` in the
    // transaction's remaining_accounts, in the same price-time-priority
    // order the book would walk. We plan fills against the resting tree,
    // then verify the caller's account list matches the plan, then apply.
    let maker_accounts = &context.remaining_accounts;
    require!(
        maker_accounts.len() % ACCOUNTS_PER_MAKER == 0,
        ErrorCode::MissingMakerAccounts
    );

    let order_book_loader = &context.accounts.order_book;

    // Plan in an immutable load scope, copy the fills out, then drop the
    // borrow before we re-borrow for mutations. AccountLoader::load() is a
    // RefCell-based runtime borrow, so the loaded ref must not outlive the
    // plan we copy out of it.
    let (fills, taker_remaining) = {
        let order_book = order_book_loader.load()?;
        plan_fills(&order_book, side, price, quantity)
    };

    require!(
        maker_accounts.len() >= fills.len() * ACCOUNTS_PER_MAKER,
        ErrorCode::MissingMakerAccounts
    );

    // Sanity-check the caller-supplied maker account list against the plan.
    // Each fill's maker_order_id must match the order_id stored on the
    // corresponding `Order` PDA, and that PDA's `market` must match this
    // market.
    for (fill_index, fill) in fills.iter().enumerate() {
        let maker_order_info = &maker_accounts[fill_index * ACCOUNTS_PER_MAKER];
        let maker_order = Account::<Order>::try_from(maker_order_info)?;
        require!(
            maker_order.order_id == fill.maker_order_id,
            ErrorCode::MakerAccountMismatch
        );
        require!(
            maker_order.market == market.key(),
            ErrorCode::MakerAccountMismatch
        );
    }

    // ---------------------------------------------------------------
    // Apply fills
    // ---------------------------------------------------------------

    let taker_market_user = &mut context.accounts.market_user;
    let mut taker_base_received: u64 = 0;
    let mut taker_quote_rebate: u64 = 0;
    let mut taker_quote_received: u64 = 0;
    // Aggregate the per-fill fee into a single transfer at the end —
    // halves CU cost vs one CPI per fill.
    let mut total_fee_quote: u64 = 0;

    for (fill_index, fill) in fills.iter().enumerate() {
        let maker_order_info = &maker_accounts[fill_index * ACCOUNTS_PER_MAKER];
        let maker_user_info = &maker_accounts[fill_index * ACCOUNTS_PER_MAKER + 1];

        let mut maker_order = Account::<Order>::try_from(maker_order_info)?;
        let mut maker_market_user = Account::<MarketUser>::try_from(maker_user_info)?;

        require!(
            maker_order.owner == maker_market_user.owner,
            ErrorCode::MakerOwnerMismatch
        );
        require!(
            maker_market_user.market == market.key(),
            ErrorCode::MakerAccountMismatch
        );

        // Fee model (simple, maker-funded, no extra taker deposit):
        //
        //   gross  = fill_price * fill_quantity (quote tokens per fill)
        //   fee    = gross * fee_bps / 10_000   (rounded down)
        //   maker gets gross - fee,
        //   fee_vault gets fee,
        //   taker pays 'gross' net (out of their pre-locked quote).
        //
        // Strictly "makers pay nothing" would require the taker to bring
        // (gross + fee) which means pulling more from the taker's ATA on
        // every fill — a per-fill CPI that inflates CU cost and account
        // lists. Real CLOBs (Openbook v2, Phoenix) use a similar
        // deduct-from-gross pattern for simplicity; the fee can be thought
        // of as the maker pricing their ask a fraction higher to cover it.
        // fill_price * fill_quantity in u128 to avoid u64-overflow on big
        // fills (high-decimal mints scale this product fast). Narrow back
        // to u64 with try_into so the transfer/balance step gets a real
        // u64 and the failure mode is explicit.
        let gross_quote: u64 = (fill.fill_price as u128)
            .checked_mul(fill.fill_quantity as u128)
            .ok_or(ErrorCode::NumericalOverflow)?
            .try_into()
            .map_err(|_| error!(ErrorCode::NumericalOverflow))?;

        let fee_quote: u64 = (gross_quote as u128)
            .checked_mul(market.fee_basis_points as u128)
            .ok_or(ErrorCode::NumericalOverflow)?
            .checked_div(BASIS_POINTS_DENOMINATOR)
            .ok_or(ErrorCode::NumericalOverflow)?
            .try_into()
            .map_err(|_| error!(ErrorCode::NumericalOverflow))?;

        // Defensive invariant: fees are a fraction of gross, never more.
        // `fee_basis_points <= 10_000` is enforced at market init, so this
        // should be unreachable — but a stale assumption here would let a
        // misconfigured market overdraw the maker's net payout. Cheap check.
        require!(fee_quote <= gross_quote, ErrorCode::NumericalOverflow);

        match side {
            // Taker Bid, resting Ask. Taker pays quote, gets base.
            OrderSide::Bid => {
                let net_quote_to_maker = gross_quote
                    .checked_sub(fee_quote)
                    .ok_or(ErrorCode::NumericalOverflow)?;
                maker_market_user.unsettled_quote = maker_market_user
                    .unsettled_quote
                    .checked_add(net_quote_to_maker)
                    .ok_or(ErrorCode::NumericalOverflow)?;

                let base_from_fill: u64 = (fill.fill_quantity as u128)
                    .checked_mul(market.base_lot_size as u128)
                    .ok_or(ErrorCode::NumericalOverflow)?
                    .try_into()
                    .map_err(|_| error!(ErrorCode::NumericalOverflow))?;
                taker_base_received = taker_base_received
                    .checked_add(base_from_fill)
                    .ok_or(ErrorCode::NumericalOverflow)?;

                // Price improvement: taker locked (price * quantity) but
                // only needs (fill_price * fill_quantity) for this fill.
                // u128 intermediate for the same reason as the bid lock
                // and gross_quote above — the original lock is already
                // bounded to u64, so this product narrows back cleanly.
                let locked_for_this_fill: u64 = (price as u128)
                    .checked_mul(fill.fill_quantity as u128)
                    .ok_or(ErrorCode::NumericalOverflow)?
                    .try_into()
                    .map_err(|_| error!(ErrorCode::NumericalOverflow))?;
                let rebate: u64 = locked_for_this_fill
                    .checked_sub(gross_quote)
                    .ok_or(ErrorCode::NumericalOverflow)?;
                taker_quote_rebate = taker_quote_rebate
                    .checked_add(rebate)
                    .ok_or(ErrorCode::NumericalOverflow)?;
            }
            // Taker Ask, resting Bid. Taker gives base, gets quote.
            OrderSide::Ask => {
                let base_from_fill: u64 = (fill.fill_quantity as u128)
                    .checked_mul(market.base_lot_size as u128)
                    .ok_or(ErrorCode::NumericalOverflow)?
                    .try_into()
                    .map_err(|_| error!(ErrorCode::NumericalOverflow))?;
                maker_market_user.unsettled_base = maker_market_user
                    .unsettled_base
                    .checked_add(base_from_fill)
                    .ok_or(ErrorCode::NumericalOverflow)?;

                let net_quote_to_taker = gross_quote
                    .checked_sub(fee_quote)
                    .ok_or(ErrorCode::NumericalOverflow)?;
                taker_quote_received = taker_quote_received
                    .checked_add(net_quote_to_taker)
                    .ok_or(ErrorCode::NumericalOverflow)?;
            }
        }

        total_fee_quote = total_fee_quote
            .checked_add(fee_quote)
            .ok_or(ErrorCode::NumericalOverflow)?;

        // Update the maker Order: bump filled_quantity, flip status.
        maker_order.filled_quantity = maker_order
            .filled_quantity
            .checked_add(fill.fill_quantity)
            .ok_or(ErrorCode::NumericalOverflow)?;

        let maker_fully_filled =
            maker_order.filled_quantity >= maker_order.original_quantity;
        maker_order.status = if maker_fully_filled {
            OrderStatus::Filled
        } else {
            OrderStatus::PartiallyFilled
        };
        if maker_fully_filled {
            remove_open_order(&mut maker_market_user, maker_order.order_id);
        }

        maker_order.exit(context.program_id)?;
        maker_market_user.exit(context.program_id)?;
    }

    // ---------------------------------------------------------------
    // Apply book-side updates from the planned fills (decrement remaining
    // qty / remove fully-filled leaves), then insert any taker remainder.
    // ---------------------------------------------------------------
    let maker_side = match side {
        OrderSide::Bid => OrderSide::Ask,
        OrderSide::Ask => OrderSide::Bid,
    };

    {
        let mut order_book = order_book_loader.load_mut()?;
        for fill in &fills {
            order_book.apply_fill_to_maker(
                maker_side,
                fill.maker_order_id,
                fill.fill_price,
                fill.fill_quantity,
            )?;
        }
    }

    // Move accumulated fee from quote_vault → fee_vault (one CPI signed
    // by the market PDA).
    if total_fee_quote > 0 {
        let market_bump = [market.bump];
        let signer_seeds: [&[u8]; 4] = [
            MARKET_SEED,
            market.base_mint.as_ref(),
            market.quote_mint.as_ref(),
            &market_bump,
        ];
        let signer_seeds = &[&signer_seeds[..]];

        transfer_checked(
            CpiContext::new_with_signer(
                context.accounts.token_program.key(),
                TransferChecked {
                    from: context.accounts.quote_vault.to_account_info(),
                    mint: context.accounts.quote_mint.to_account_info(),
                    to: context.accounts.fee_vault.to_account_info(),
                    authority: market.to_account_info(),
                },
                signer_seeds,
            ),
            total_fee_quote,
            context.accounts.quote_mint.decimals,
        )?;
    }

    // Apply taker accounting deltas in a single mutation.
    taker_market_user.unsettled_base = taker_market_user
        .unsettled_base
        .checked_add(taker_base_received)
        .ok_or(ErrorCode::NumericalOverflow)?;
    taker_market_user.unsettled_quote = taker_market_user
        .unsettled_quote
        .checked_add(taker_quote_rebate)
        .ok_or(ErrorCode::NumericalOverflow)?
        .checked_add(taker_quote_received)
        .ok_or(ErrorCode::NumericalOverflow)?;

    // ---------------------------------------------------------------
    // Stamp the taker's Order PDA. Either Filled (no remainder) or rested.
    //
    // The seq_num we baked into the order_id (via `next_order_id` at the
    // top of the planning section) is what the slab uses as the time-
    // priority tie-break. We allocate it inside the load_mut scope below
    // so the counter and the leaf insert happen against the same view of
    // the book.
    // ---------------------------------------------------------------
    let timestamp = Clock::get()?.unix_timestamp;
    let order_id = {
        let mut order_book = order_book_loader.load_mut()?;
        let id = order_book.allocate_order_id()?;
        if taker_remaining > 0 {
            require!(
                !order_book.is_side_full(side),
                ErrorCode::OrderBookFull
            );
            order_book.place_resting(
                side,
                price,
                taker_remaining,
                context.accounts.owner.key(),
                id,
                timestamp,
            )?;
        }
        id
    };

    let order = &mut context.accounts.order;
    order.market = market.key();
    order.owner = context.accounts.owner.key();
    order.order_id = order_id;
    order.side = side;
    order.price = price;
    order.original_quantity = quantity;
    // checked_sub, not saturating_sub: a silent clamp on filled_quantity
    // would be a real correctness bug (the matching engine should never
    // hand us a remainder larger than the original). saturating_* belongs
    // on cosmetic/UX paths, not on a field the matching engine relies on.
    order.filled_quantity = quantity
        .checked_sub(taker_remaining)
        .ok_or(ErrorCode::NumericalOverflow)?;
    order.timestamp = timestamp;
    order.bump = context.bumps.order;

    if taker_remaining == 0 {
        order.status = OrderStatus::Filled;
    } else {
        order.status = if taker_remaining < quantity {
            OrderStatus::PartiallyFilled
        } else {
            OrderStatus::Open
        };
        add_open_order(taker_market_user, order_id);
    }

    Ok(())
}

#[derive(Accounts)]
#[instruction(side: OrderSide, price: u64, quantity: u64)]
pub struct PlaceOrder<'info> {
    // `has_one` ties every market-owned account on this struct to the
    // addresses recorded on the Market PDA. Crucially, without
    // has_one on base_vault / quote_vault / base_mint / quote_mint a caller
    // could swap fee_vault in for quote_vault (same mint, same authority)
    // and steer the per-fill fee transfer to drain real fees instead of
    // routing them in.
    #[account(
        mut,
        has_one = fee_vault @ ErrorCode::InvalidFeeVault,
        has_one = base_vault @ ErrorCode::InvalidBaseVault,
        has_one = quote_vault @ ErrorCode::InvalidQuoteVault,
        has_one = base_mint @ ErrorCode::InvalidBaseMint,
        has_one = quote_mint @ ErrorCode::InvalidQuoteMint,
        has_one = order_book @ ErrorCode::InvalidOrderBook,
    )]
    pub market: Account<'info, Market>,

    // Zero-copy: AccountLoader streams the slab in/out without paying
    // borsh (de)serialization on every instruction. See order_book.rs for
    // the layout. Not a PDA — the client created it directly via
    // system_program::create_account (see initialize_market.rs for why);
    // `has_one = order_book` on `market` is what ties this specific account
    // to this specific market.
    #[account(mut)]
    pub order_book: AccountLoader<'info, OrderBook>,

    // The order PDA seed uses the book's `next_order_id` *before* this
    // instruction increments it — i.e. the id this new order will receive.
    // Read via `load()` so Anchor can derive the PDA at verification time.
    #[account(
        init,
        payer = owner,
        space = Order::DISCRIMINATOR.len() + Order::INIT_SPACE,
        seeds = [
            ORDER_SEED,
            market.key().as_ref(),
            order_book.load()?.next_order_id.to_le_bytes().as_ref()
        ],
        bump
    )]
    pub order: Account<'info, Order>,

    #[account(
        mut,
        seeds = [MARKET_USER_SEED, market.key().as_ref(), owner.key().as_ref()],
        bump = market_user.bump
    )]
    pub market_user: Account<'info, MarketUser>,

    // InterfaceAccount on the stack is ~1 KB each; with 7 of them this struct
    // blows the 4 KB stack-offset limit on BPF. Boxing moves each to the heap.
    #[account(mut)]
    pub base_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut)]
    pub quote_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    // Taker fees are routed here. Constrained via `has_one = fee_vault` on
    // `market` above so the program can trust it without re-checking.
    #[account(mut)]
    pub fee_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut)]
    pub user_base_account: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut)]
    pub user_quote_account: Box<InterfaceAccount<'info, TokenAccount>>,

    pub base_mint: Box<InterfaceAccount<'info, Mint>>,

    pub quote_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(mut)]
    pub owner: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,

    pub system_program: Program<'info, System>,
}
