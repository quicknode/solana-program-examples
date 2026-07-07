use quasar_lang::prelude::*;
use quasar_lang::sysvars::Sysvar as _;
use quasar_spl::prelude::*;

use crate::errors::VaultError;
use crate::state::{snapshot_strategy, ShareMintPda, Strategy, STRATEGY_SEED};

const SECONDS_PER_YEAR: u64 = 31_536_000;

#[derive(Accounts)]
pub struct CollectFeesAccountConstraints {
    /// Read-only: the manager is stored on the strategy; fees are minted to
    /// their share account. Not a signer - anyone may trigger accrual.
    pub manager: UncheckedAccount,

    #[account(mut, address = Strategy::seeds(strategy.index.into()), has_one(manager))]
    pub strategy: Account<Strategy>,

    #[account(mut, address = ShareMintPda::seeds(strategy.address()))]
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
        accounts.strategy.manager,
        VaultError::InvalidRecipient
    );

    let now = i64::from(Clock::get()?.unix_timestamp);
    let last = i64::from(accounts.strategy.last_fee_accrual_timestamp);
    require!(now > last, VaultError::NoTimeElapsed);

    let elapsed_seconds = (now - last) as u64;
    let total_shares = u64::from(accounts.strategy.total_shares);
    let fee_bps = u16::from(accounts.strategy.fee_bps);
    let strategy_index = u64::from(accounts.strategy.index);
    let strategy_bump = accounts.strategy.bump;

    // fee_shares = total_shares * fee_bps * elapsed / (10_000 * SECONDS_PER_YEAR)
    let denominator = (10_000u128)
        .checked_mul(SECONDS_PER_YEAR as u128)
        .ok_or(VaultError::MathOverflow)?;
    let fee_shares: u64 = (total_shares as u128)
        .checked_mul(fee_bps as u128)
        .ok_or(VaultError::MathOverflow)?
        .checked_mul(elapsed_seconds as u128)
        .ok_or(VaultError::MathOverflow)?
        .checked_div(denominator)
        .ok_or(VaultError::MathOverflow)?
        .try_into()
        .map_err(|_| VaultError::MathOverflow)?;

    // Advance the accrual clock even when the fee rounds to zero.
    let mut strategy = snapshot_strategy(&accounts.strategy);
    strategy.last_fee_accrual_timestamp = now;
    if fee_shares == 0 {
        accounts.strategy.set_inner(strategy);
        return Ok(());
    }
    strategy.total_shares = total_shares
        .checked_add(fee_shares)
        .ok_or(VaultError::MathOverflow)?;
    accounts.strategy.set_inner(strategy);

    // Mint fee shares to the manager; the strategy PDA signs as mint authority.
    let index_bytes = strategy_index.to_le_bytes();
    let bump = [strategy_bump];
    let seeds = [
        Seed::from(STRATEGY_SEED),
        Seed::from(index_bytes.as_ref()),
        Seed::from(bump.as_ref()),
    ];
    accounts
        .token_program
        .mint_to(
            &accounts.share_mint,
            &accounts.manager_share_account,
            &accounts.strategy,
            fee_shares,
        )
        .invoke_signed(&seeds)?;

    Ok(())
}
