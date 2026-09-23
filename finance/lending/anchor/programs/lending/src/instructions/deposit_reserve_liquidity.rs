use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    mint_to, transfer_checked, Mint, MintTo, TokenAccount, TokenInterface, TransferChecked,
};

use crate::constants::MINIMUM_SHARES;
use crate::errors::LendingError;
use crate::math::mul_div_floor;
use crate::state::{reserve_signer_seeds, Reserve};

/// Supply liquidity to a reserve and receive share tokens. The first deposit
/// mints share tokens 1:1, less the `MINIMUM_SHARES` withheld; later deposits
/// mint `liquidity_amount * total_shares / total_liquidity`, where
/// `total_shares` counts the withheld minimum, floored so the protocol keeps
/// any rounding dust.
pub fn handle_deposit_reserve_liquidity(
    context: &mut Context<DepositReserveLiquidity>,
    liquidity_amount: u64,
) -> Result<()> {
    require!(liquidity_amount > 0, LendingError::ZeroAmount);
    let reserve = &mut context.accounts.reserve;
    reserve.require_refreshed()?;

    let total_liquidity = reserve.total_liquidity()?;
    let share_amount = if reserve.share_mint_supply == 0 && total_liquidity == 0 {
        // Bootstrap: shares track liquidity one-for-one, less the withheld
        // minimum, so the share supply can never start at a dust amount.
        liquidity_amount
            .checked_sub(MINIMUM_SHARES)
            .ok_or(LendingError::DepositTooSmall)? as u128
    } else {
        // The withheld minimum counts as shares nobody holds, here and in every
        // other conversion, so its slice of the pool is locked for good. That
        // is what stops share inflation: a lone supplier who borrows from their
        // own reserve can lift `total_liquidity` with the interest they owe,
        // and deposits and redemptions that round in the pool's favour lift it
        // further, until one share is worth enough to round a later deposit
        // down. With the minimum counted, their one share is 1 of 1_001 and
        // whatever they leave in the pool goes mostly to shares nobody redeems.
        //
        // A reserve whose suppliers have all left takes this branch too: the
        // minimum's slice is still in `total_liquidity`, so the next deposit
        // is priced against it rather than bootstrapped.
        mul_div_floor(
            liquidity_amount as u128,
            reserve.total_shares()?,
            total_liquidity,
        )?
    };
    require!(share_amount > 0, LendingError::DepositTooSmall);
    let share_amount = u64::try_from(share_amount).map_err(|_| LendingError::MathOverflow)?;

    // Effects before interactions.
    reserve.available_liquidity = reserve
        .available_liquidity
        .checked_add(liquidity_amount)
        .ok_or(LendingError::MathOverflow)?;
    reserve.share_mint_supply = reserve
        .share_mint_supply
        .checked_add(share_amount)
        .ok_or(LendingError::MathOverflow)?;

    transfer_checked(
        CpiContext::new(
            context.accounts.token_program.address(),
            TransferChecked {
                from: context.accounts.user_liquidity.cpi_handle_mut(),
                mint: context.accounts.liquidity_mint.cpi_handle(),
                to: context.accounts.liquidity_vault.cpi_handle_mut(),
                authority: context.accounts.owner.cpi_handle(),
            },
        ),
        liquidity_amount,
        reserve.liquidity_decimals,
    )?;

    // Copy the seed inputs out: `release_borrow` below needs `&mut reserve`.
    let bump = [reserve.bump];
    let lending_market = reserve.lending_market;
    let liquidity_mint = reserve.liquidity_mint;
    let seeds = reserve_signer_seeds(&lending_market, &liquidity_mint, &bump);
    // `reserve` signs this CPI. It is a data account holding a live borrow on
    // its buffer, which the runtime would reject when the CPI borrows the same
    // account, so hand the borrow back across the call. `release_borrow`
    // flushes the pending writes, and `reacquire_borrow_mut` re-reads them.
    context.accounts.reserve.release_borrow()?;
    mint_to(
        CpiContext::new_with_signer(
            context.accounts.token_program.address(),
            MintTo {
                mint: context.accounts.share_mint.cpi_handle_mut(),
                to: context.accounts.user_share.cpi_handle_mut(),
                authority: context.accounts.reserve.cpi_handle(),
            },
            &[&seeds],
        ),
        share_amount,
    )?;
    context.accounts.reserve.reacquire_borrow_mut()?;

    Ok(())
}

#[derive(Accounts)]
pub struct DepositReserveLiquidity {
    #[account(mut)]
    pub reserve: BorshAccount<Reserve>,

    #[account(address = reserve.liquidity_mint)]
    pub liquidity_mint: InterfaceAccount<Mint>,

    #[account(mut, address = reserve.liquidity_vault)]
    pub liquidity_vault: InterfaceAccount<TokenAccount>,

    #[account(mut, address = reserve.share_mint)]
    pub share_mint: InterfaceAccount<Mint>,

    #[account(mut)]
    pub user_liquidity: InterfaceAccount<TokenAccount>,

    #[account(mut)]
    pub user_share: InterfaceAccount<TokenAccount>,

    pub owner: Signer,

    pub token_program: Interface<'static, TokenInterface>,
}
