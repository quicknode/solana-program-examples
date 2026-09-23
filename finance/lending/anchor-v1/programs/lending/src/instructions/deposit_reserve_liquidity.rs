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
    context: Context<DepositReserveLiquidity>,
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
            context.accounts.token_program.key(),
            TransferChecked {
                from: context.accounts.user_liquidity.to_account_info(),
                mint: context.accounts.liquidity_mint.to_account_info(),
                to: context.accounts.liquidity_vault.to_account_info(),
                authority: context.accounts.owner.to_account_info(),
            },
        ),
        liquidity_amount,
        reserve.liquidity_decimals,
    )?;

    let bump = [reserve.bump];
    let seeds = reserve_signer_seeds(&reserve.lending_market, &reserve.liquidity_mint, &bump);
    mint_to(
        CpiContext::new_with_signer(
            context.accounts.token_program.key(),
            MintTo {
                mint: context.accounts.share_mint.to_account_info(),
                to: context.accounts.user_share.to_account_info(),
                authority: reserve.to_account_info(),
            },
            &[&seeds],
        ),
        share_amount,
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct DepositReserveLiquidity<'info> {
    #[account(
        mut,
        has_one = liquidity_mint,
        has_one = liquidity_vault,
        has_one = share_mint,
    )]
    pub reserve: Account<'info, Reserve>,

    pub liquidity_mint: InterfaceAccount<'info, Mint>,

    #[account(mut)]
    pub liquidity_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub share_mint: InterfaceAccount<'info, Mint>,

    #[account(mut)]
    pub user_liquidity: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub user_share: InterfaceAccount<'info, TokenAccount>,

    pub owner: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}
