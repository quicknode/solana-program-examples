use anchor_lang::prelude::*;

use crate::constants::BPS_DENOMINATOR;
use crate::errors::LendingError;
use crate::math::{market_value, mul_div_ceil, mul_div_floor, Rounding};
use crate::state::{Obligation, PriceFeed, Reserve};

/// Recompute the obligation's deposited/borrowed values and its borrow limits
/// from the current state of every reserve it touches.
///
/// The reserve and price-feed accounts are passed as `remaining_accounts`, two
/// per entry — first the deposit reserves in `obligation.deposits` order, then
/// the borrow reserves in `obligation.borrows` order — each as
/// `[reserve, price_feed]`. Every reserve must already be refreshed this slot.
///
/// Collateral value is floored and debt value is ceiled, so health is always
/// evaluated conservatively against the borrower.
pub fn handle_refresh_obligation(context: &mut Context<RefreshObligation>) -> Result<()> {
    let slot = Clock::get()?.slot;
    let obligation = &mut context.accounts.obligation;
    let lending_market = obligation.lending_market;
    let accounts = context.remaining_accounts()?;
    let mut cursor = 0usize;

    let mut deposited_value: u128 = 0;
    let mut allowed_borrow_value: u128 = 0;
    let mut unhealthy_borrow_value: u128 = 0;

    for collateral in obligation.deposits.iter_mut() {
        let (reserve, price_scaled) = read_pair(
            accounts,
            &mut cursor,
            collateral.reserve,
            lending_market,
            slot,
        )?;

        let liquidity = mul_div_floor(
            collateral.deposited_shares as u128,
            reserve.total_liquidity()?,
            (reserve.share_mint_supply as u128).max(1),
        )?;
        let liquidity = u64::try_from(liquidity).map_err(|_| LendingError::MathOverflow)?;
        let value = market_value(
            liquidity,
            reserve.liquidity_decimals,
            price_scaled,
            Rounding::Down,
        )?;

        collateral.market_value = value;
        deposited_value = deposited_value
            .checked_add(value)
            .ok_or(LendingError::MathOverflow)?;
        allowed_borrow_value = allowed_borrow_value
            .checked_add(mul_div_floor(
                value,
                reserve.config.loan_to_value_bps as u128,
                BPS_DENOMINATOR,
            )?)
            .ok_or(LendingError::MathOverflow)?;
        unhealthy_borrow_value = unhealthy_borrow_value
            .checked_add(mul_div_floor(
                value,
                reserve.config.liquidation_threshold_bps as u128,
                BPS_DENOMINATOR,
            )?)
            .ok_or(LendingError::MathOverflow)?;
    }

    let mut borrowed_value: u128 = 0;
    for borrow in obligation.borrows.iter_mut() {
        let (reserve, price_scaled) =
            read_pair(accounts, &mut cursor, borrow.reserve, lending_market, slot)?;

        let debt = mul_div_ceil(
            borrow.borrowed_principal,
            reserve.borrow_accumulation_factor,
            crate::constants::FIXED_POINT_SCALE,
        )?;
        let debt = u64::try_from(debt).map_err(|_| LendingError::MathOverflow)?;
        let value = market_value(debt, reserve.liquidity_decimals, price_scaled, Rounding::Up)?;

        borrow.market_value = value;
        borrowed_value = borrowed_value
            .checked_add(value)
            .ok_or(LendingError::MathOverflow)?;
    }

    require!(
        cursor == accounts.len(),
        LendingError::InvalidObligationAccount
    );

    obligation.deposited_value = deposited_value;
    obligation.allowed_borrow_value = allowed_borrow_value;
    obligation.unhealthy_borrow_value = unhealthy_borrow_value;
    obligation.borrowed_value = borrowed_value;
    obligation.last_update_slot = slot;
    obligation.stale = false;
    Ok(())
}

/// Read the next `[reserve, price_feed]` pair from `remaining_accounts`,
/// checking it matches the obligation's stored reserve, belongs to the
/// obligation's lending market, and that both the reserve (refreshed this
/// slot) and the price (fresh) are usable.
fn read_pair<'a, 'info>(
    accounts: &'a [AccountView],
    cursor: &mut usize,
    expected_reserve: Address,
    lending_market: Address,
    slot: u64,
) -> Result<(Reserve, u128)>
where
    'a: 'info,
{
    let reserve_info = accounts
        .get(*cursor)
        .ok_or(LendingError::InvalidObligationAccount)?;
    let price_info = accounts
        .get(*cursor + 1)
        .ok_or(LendingError::InvalidObligationAccount)?;
    *cursor += 2;

    require_keys_eq!(
        *reserve_info.address(),
        expected_reserve,
        LendingError::InvalidObligationAccount
    );
    let reserve = {
        let data = reserve_info.try_borrow()?;
        let disc_len = <Reserve as anchor_lang::Discriminator>::DISCRIMINATOR.len();
        require!(
            data.len() > disc_len
                && &data[..disc_len] == <Reserve as anchor_lang::Discriminator>::DISCRIMINATOR,
            LendingError::InvalidObligationAccount
        );
        let mut payload = &data[disc_len..];
        <Reserve as wincode::SchemaRead<anchor_lang::BorshConfig>>::get(&mut payload)
            .map_err(|_| LendingError::InvalidObligationAccount)?
    };
    require_keys_eq!(
        reserve.lending_market,
        lending_market,
        LendingError::MarketMismatch
    );
    reserve.require_refreshed()?;

    require_keys_eq!(
        *price_info.address(),
        reserve.price_feed,
        LendingError::InvalidObligationAccount
    );
    let price_feed = Account::<PriceFeed>::try_from(price_info)?;
    let price_scaled = price_feed.price_scaled(slot)?;

    Ok((reserve.into_inner(), price_scaled))
}

#[derive(Accounts)]
pub struct RefreshObligation {
    #[account(mut)]
    pub obligation: BorshAccount<Obligation>,
}
