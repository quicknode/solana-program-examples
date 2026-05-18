use {
    crate::{
        constants::{COLLATERAL_VAULT_SEED, LEASE_SEED},
        errors::AssetLeasingError,
        state::{Lease, LeaseStatus},
    },
    quasar_lang::prelude::*,
    quasar_spl::{Mint, Token, TokenCpi},
};

/// Accounts for increasing collateral on an `Active` lease. Only the
/// registered lessee may call — anyone else hitting the program returns
/// `Unauthorised`.
#[derive(Accounts)]
pub struct TopUpCollateral<'info> {
    #[account(mut)]
    pub lessee: &'info Signer,

    /// program-derived address seed only — not read directly.
    pub lessor: &'info UncheckedAccount,

    #[account(
        mut,
        seeds = [LEASE_SEED, lessor],
        bump = lease.bump,
        has_one = lessor,
        has_one = collateral_mint,
        constraint = lease.lessee == *lessee.address() @ AssetLeasingError::Unauthorised,
        constraint = LeaseStatus::from_u8(lease.status) == Some(LeaseStatus::Active)
            @ AssetLeasingError::InvalidLeaseStatus,
    )]
    pub lease: &'info mut Account<Lease>,

    pub collateral_mint: &'info Account<Mint>,

    #[account(
        mut,
        seeds = [COLLATERAL_VAULT_SEED, lease],
        bump = lease.collateral_vault_bump,
    )]
    pub collateral_vault: &'info mut Account<Token>,

    #[account(mut)]
    pub lessee_collateral_account: &'info mut Account<Token>,

    pub token_program: &'info Program<Token>,
}

#[inline(always)]
pub fn handle_top_up_collateral(
    accounts: &mut TopUpCollateral,
    amount: u64,
) -> Result<(), ProgramError> {
    require!(amount > 0, AssetLeasingError::InvalidCollateralAmount);

    accounts
        .token_program
        .transfer(
            accounts.lessee_collateral_account,
            accounts.collateral_vault,
            accounts.lessee,
            amount,
        )
        .invoke()?;

    let new_collateral = accounts
        .lease
        .collateral_amount
        .get()
        .checked_add(amount)
        .ok_or(AssetLeasingError::MathOverflow)?;
    accounts.lease.collateral_amount = new_collateral.into();

    Ok(())
}
