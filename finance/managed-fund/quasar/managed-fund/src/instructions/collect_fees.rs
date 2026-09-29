use quasar_lang::cpi::Seed;
use quasar_lang::prelude::*;
use quasar_lang::sysvars::Sysvar as _;
use quasar_spl::prelude::*;

use crate::errors::FundError;
use crate::state::{snapshot_fund, Fund, ShareMintPda, FUND_SEED};

const SECONDS_PER_YEAR: u64 = 31_536_000;

#[derive(Accounts)]
pub struct CollectFeesAccountConstraints {
    /// Read-only: the manager is stored on the fund; fees are minted to
    /// their share account. Not a signer - anyone may trigger accrual.
    pub manager: UncheckedAccount,

    #[account(mut, address = Fund::seeds(fund.index.into()), has_one(manager))]
    pub fund: Account<Fund>,

    #[account(mut, address = ShareMintPda::seeds(fund.address()))]
    pub share_mint: InterfaceAccount<Mint>,

    /// The manager's share token account - receives fee shares.
    #[account(mut)]
    pub manager_share_account: Account<Token>,

    #[account(mut)]
    pub payer: Signer,

    pub token_program: Program<TokenProgram>,
}

#[inline(always)]
pub fn handle_collect_fees(
    accounts: &mut CollectFeesAccountConstraints,
) -> Result<(), ProgramError> {
    require_keys_eq!(
        accounts.manager_share_account.owner,
        accounts.fund.manager,
        FundError::InvalidRecipient
    );

    let now = i64::from(Clock::get()?.unix_timestamp);
    let last = i64::from(accounts.fund.last_fee_accrual_timestamp);
    require!(now > last, FundError::NoTimeElapsed);

    let elapsed_seconds = (now - last) as u64;
    let total_shares = u64::from(accounts.fund.total_shares);
    let fee_bps = u16::from(accounts.fund.fee_bps);
    let fund_index = u64::from(accounts.fund.index);
    let fund_bump = accounts.fund.bump;

    // fee_shares = total_shares * fee_bps * elapsed / (10_000 * SECONDS_PER_YEAR)
    let denominator = (10_000u128)
        .checked_mul(SECONDS_PER_YEAR as u128)
        .ok_or(FundError::MathOverflow)?;
    let fee_shares: u64 = (total_shares as u128)
        .checked_mul(fee_bps as u128)
        .ok_or(FundError::MathOverflow)?
        .checked_mul(elapsed_seconds as u128)
        .ok_or(FundError::MathOverflow)?
        .checked_div(denominator)
        .ok_or(FundError::MathOverflow)?
        .try_into()
        .map_err(|_| FundError::MathOverflow)?;

    // Advance the accrual clock even when the fee rounds to zero.
    let mut fund = snapshot_fund(&accounts.fund);
    fund.last_fee_accrual_timestamp = now;
    if fee_shares == 0 {
        accounts.fund.set_inner(fund);
        return Ok(());
    }
    fund.total_shares = total_shares
        .checked_add(fee_shares)
        .ok_or(FundError::MathOverflow)?;
    accounts.fund.set_inner(fund);

    // Mint fee shares to the manager; the fund PDA signs as mint authority.
    let index_bytes = fund_index.to_le_bytes();
    let bump = [fund_bump];
    let seeds = [
        Seed::from(FUND_SEED),
        Seed::from(index_bytes.as_ref()),
        Seed::from(bump.as_ref()),
    ];
    accounts
        .token_program
        .mint_to(
            &accounts.share_mint,
            &accounts.manager_share_account,
            &accounts.fund,
            fee_shares,
        )
        .invoke_signed(&seeds)?;

    Ok(())
}
