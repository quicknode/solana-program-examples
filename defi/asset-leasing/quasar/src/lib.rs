#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

mod constants;
mod errors;
mod instructions;
mod state;

use instructions::*;
#[cfg(test)]
mod tests;

// Same program id as the Anchor version so off-chain tooling that derives
// program-derived addresses or looks up the program onchain works against both binaries
// interchangeably.
declare_id!("Lease11111111111111111111111111111111111111");

/// Asset-leasing program: fixed-term token leases with a streaming lease fee
/// payment, collateral escrow, and Pyth-oracle-triggered liquidation.
///
/// See the top-level `defi/asset-leasing/anchor/README.md` for the full
/// mechanics — the Quasar and Anchor versions are functionally identical.
#[program]
mod quasar_asset_leasing {
    use super::*;

    /// Discriminators are packed densely starting from 0 so the wire format
    /// stays a single byte. The order matches the natural user-facing
    /// lifecycle (create → take → pay/top-up → return/liquidate/close).
    #[instruction(discriminator = 0)]
    pub fn create_lease(
        context: Ctx<CreateLease>,
        lease_id: u64,
        leased_amount: u64,
        required_collateral_amount: u64,
        lease_fee_per_second: u64,
        duration_seconds: i64,
        maintenance_margin_basis_points: u16,
        liquidation_bounty_basis_points: u16,
        feed_id: [u8; 32],
    ) -> Result<(), ProgramError> {
        instructions::handle_create_lease(
            &mut context.accounts,
            lease_id,
            leased_amount,
            required_collateral_amount,
            lease_fee_per_second,
            duration_seconds,
            maintenance_margin_basis_points,
            liquidation_bounty_basis_points,
            feed_id,
            &context.bumps,
        )
    }

    #[instruction(discriminator = 1)]
    pub fn take_lease(context: Ctx<TakeLease>) -> Result<(), ProgramError> {
        instructions::handle_take_lease(&mut context.accounts)
    }

    #[instruction(discriminator = 2)]
    pub fn pay_lease_fee(context: Ctx<PayLeaseFee>) -> Result<(), ProgramError> {
        instructions::handle_pay_lease_fee(&mut context.accounts)
    }

    #[instruction(discriminator = 3)]
    pub fn top_up_collateral(
        context: Ctx<TopUpCollateral>,
        amount: u64,
    ) -> Result<(), ProgramError> {
        instructions::handle_top_up_collateral(&mut context.accounts, amount)
    }

    #[instruction(discriminator = 4)]
    pub fn return_lease(context: Ctx<ReturnLease>) -> Result<(), ProgramError> {
        instructions::handle_return_lease(&mut context.accounts)
    }

    #[instruction(discriminator = 5)]
    pub fn liquidate(context: Ctx<Liquidate>) -> Result<(), ProgramError> {
        instructions::handle_liquidate(&mut context.accounts)
    }

    #[instruction(discriminator = 6)]
    pub fn close_expired(context: Ctx<CloseExpired>) -> Result<(), ProgramError> {
        instructions::handle_close_expired(&mut context.accounts)
    }
}
