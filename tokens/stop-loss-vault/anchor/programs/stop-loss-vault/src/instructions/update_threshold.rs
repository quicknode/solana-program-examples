use anchor_lang::prelude::*;

use crate::error::StopLossError;
use crate::state::Vault;

/// Trail the stop-loss threshold up (or down) and/or change the suggested
/// crank cadence. Both arguments are optional so the owner can change one
/// without resending the other; a single combined ix keeps the on-chain
/// surface small.
///
/// Refuses to mutate once the vault has triggered: at that point the vault is
/// effectively a stable-token wallet and there's nothing left to protect.
pub fn handler(
    ctx: Context<UpdateThresholdAccountConstraints>,
    new_threshold_price: Option<i128>,
    new_crank_interval_seconds: Option<u32>,
) -> Result<()> {
    let vault = &mut ctx.accounts.vault;
    require!(!vault.triggered, StopLossError::VaultAlreadyTriggered);

    if let Some(threshold) = new_threshold_price {
        vault.threshold_price = threshold;
    }
    if let Some(interval) = new_crank_interval_seconds {
        vault.crank_interval_seconds = interval;
    }
    Ok(())
}

#[derive(Accounts)]
pub struct UpdateThresholdAccountConstraints<'info> {
    #[account(
        mut,
        seeds = [Vault::SEED_PREFIX, owner.key().as_ref()],
        bump = vault.bump,
        has_one = owner @ StopLossError::Unauthorized,
    )]
    pub vault: Account<'info, Vault>,

    pub owner: Signer<'info>,
}
