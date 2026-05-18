use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::{
    constants::{
        COLLATERAL_VAULT_SEED, LEASED_VAULT_SEED, LEASE_SEED, MAX_LIQUIDATION_BOUNTY_BASIS_POINTS,
        MAX_MAINTENANCE_MARGIN_BASIS_POINTS,
    },
    errors::AssetLeasingError,
    instructions::shared::transfer_tokens_from_user,
    state::{Lease, LeaseStatus},
};

#[derive(Accounts)]
#[instruction(lease_id: u64)]
pub struct CreateLease<'info> {
    #[account(mut)]
    pub holder: Signer<'info>,

    #[account(mint::token_program = token_program)]
    pub leased_mint: InterfaceAccount<'info, Mint>,

    #[account(mint::token_program = token_program)]
    pub collateral_mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        associated_token::mint = leased_mint,
        associated_token::authority = holder,
        associated_token::token_program = token_program,
    )]
    pub holder_leased_account: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init,
        payer = holder,
        space = Lease::DISCRIMINATOR.len() + Lease::INIT_SPACE,
        seeds = [LEASE_SEED, holder.key().as_ref(), &lease_id.to_le_bytes()],
        bump,
    )]
    pub lease: Account<'info, Lease>,

    /// program-derived address-owned vault holding the leased tokens while `Listed`. Authority is
    /// the vault program-derived address itself so the lease account does not need to sign for
    /// returns / liquidation; any handler just signs with the vault seeds.
    #[account(
        init,
        payer = holder,
        seeds = [LEASED_VAULT_SEED, lease.key().as_ref()],
        bump,
        token::mint = leased_mint,
        token::authority = leased_vault,
        token::token_program = token_program,
    )]
    pub leased_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init,
        payer = holder,
        seeds = [COLLATERAL_VAULT_SEED, lease.key().as_ref()],
        bump,
        token::mint = collateral_mint,
        token::authority = collateral_vault,
        token::token_program = token_program,
    )]
    pub collateral_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

#[allow(clippy::too_many_arguments)]
pub fn handle_create_lease(
    context: Context<CreateLease>,
    lease_id: u64,
    leased_amount: u64,
    required_collateral_amount: u64,
    lease_fee_per_second: u64,
    duration_seconds: i64,
    maintenance_margin_basis_points: u16,
    liquidation_bounty_basis_points: u16,
    feed_id: [u8; 32],
) -> Result<()> {
    // Reject leased_mint == collateral_mint. Allowing both to be the same
    // mint would collapse the two vaults' seed derivations into one shared
    // token-balance pool, making lease-fee-vs-collateral accounting ambiguous and
    // enabling griefing paths where the short_seller's "collateral" is the same
    // asset they already hold as the lease principal.
    require!(
        context.accounts.leased_mint.key() != context.accounts.collateral_mint.key(),
        AssetLeasingError::LeasedMintEqualsCollateralMint
    );

    require!(leased_amount > 0, AssetLeasingError::InvalidLeasedAmount);
    require!(
        required_collateral_amount > 0,
        AssetLeasingError::InvalidCollateralAmount
    );
    require!(lease_fee_per_second > 0, AssetLeasingError::InvalidLeaseFeePerSecond);
    require!(duration_seconds > 0, AssetLeasingError::InvalidDuration);
    require!(
        maintenance_margin_basis_points > 0 && maintenance_margin_basis_points <= MAX_MAINTENANCE_MARGIN_BASIS_POINTS,
        AssetLeasingError::InvalidMaintenanceMargin
    );
    require!(
        liquidation_bounty_basis_points <= MAX_LIQUIDATION_BOUNTY_BASIS_POINTS,
        AssetLeasingError::InvalidLiquidationBounty
    );

    // Lock the leased tokens into the program-owned vault up-front. Doing this
    // here (not on take_lease) guarantees a short_seller can never accept a lease
    // the holder no longer has the funds to deliver.
    transfer_tokens_from_user(
        &context.accounts.holder_leased_account,
        &context.accounts.leased_vault,
        leased_amount,
        &context.accounts.leased_mint,
        &context.accounts.holder,
        &context.accounts.token_program,
    )?;

    let lease = &mut context.accounts.lease;
    lease.set_inner(Lease {
        lease_id,
        holder: context.accounts.holder.key(),
        // No short_seller yet — will be populated by take_lease.
        short_seller: Pubkey::default(),
        leased_mint: context.accounts.leased_mint.key(),
        leased_amount,
        collateral_mint: context.accounts.collateral_mint.key(),
        // No collateral yet — posted on take_lease.
        collateral_amount: 0,
        required_collateral_amount,
        lease_fee_per_second,
        duration_seconds,
        // start_timestamp / end_timestamp / last_paid_timestamp are set when the lease
        // activates in `take_lease`.
        start_timestamp: 0,
        end_timestamp: 0,
        last_paid_timestamp: 0,
        maintenance_margin_basis_points,
        liquidation_bounty_basis_points,
        feed_id,
        status: LeaseStatus::Listed,
        bump: context.bumps.lease,
        leased_vault_bump: context.bumps.leased_vault,
        collateral_vault_bump: context.bumps.collateral_vault,
    });

    Ok(())
}
