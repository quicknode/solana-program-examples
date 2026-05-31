//! Stop-loss vault.
//!
//! Holds a single volatile SPL token for one owner and permissionlessly
//! converts it to a single stable SPL token when a Switchboard On-Demand
//! price feed drops below a user-set threshold. TukTuk schedules the
//! permissionless crank.
//!
//! See `README.md` (one directory up) for the architecture overview,
//! limitations, and integration notes.
pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("BSzhyK5soR2T3T1LCjwYVybff2D9NowwfFHdVsAwnkmG");

#[program]
pub mod stop_loss_vault {
    use super::*;

    /// Create a vault for the caller. See
    /// `instructions::initialize_vault::handler`.
    pub fn initialize_vault(
        ctx: Context<InitializeVaultAccountConstraints>,
        threshold_price: i128,
        crank_interval_seconds: u32,
        tuktuk_task: Pubkey,
    ) -> Result<()> {
        instructions::initialize_vault::handler(
            ctx,
            threshold_price,
            crank_interval_seconds,
            tuktuk_task,
        )
    }

    /// Owner deposits volatile tokens.
    pub fn deposit(ctx: Context<DepositAccountConstraints>, amount: u64) -> Result<()> {
        instructions::deposit::handler(ctx, amount)
    }

    /// Owner adjusts the stop-loss threshold and/or suggested crank cadence.
    /// Both fields are optional; pass `None` to leave a field unchanged.
    pub fn update_threshold(
        ctx: Context<UpdateThresholdAccountConstraints>,
        new_threshold_price: Option<i128>,
        new_crank_interval_seconds: Option<u32>,
    ) -> Result<()> {
        instructions::update_threshold::handler(
            ctx,
            new_threshold_price,
            new_crank_interval_seconds,
        )
    }

    /// Permissionless conversion — see
    /// `instructions::convert_if_triggered::handler` for the flash-crash
    /// limitation documented at the API level.
    pub fn convert_if_triggered(
        ctx: Context<ConvertIfTriggeredAccountConstraints>,
        switchboard_price_update_data: Vec<u8>,
    ) -> Result<()> {
        instructions::convert_if_triggered::handler(ctx, switchboard_price_update_data)
    }

    /// Owner withdraws stables after a trigger.
    pub fn withdraw_stables(
        ctx: Context<WithdrawStablesAccountConstraints>,
        amount: u64,
    ) -> Result<()> {
        instructions::withdraw_stables::handler(ctx, amount)
    }

    /// Owner withdraws volatile tokens before a trigger — the escape hatch for
    /// a vault that never fires. See `instructions::withdraw_volatile::handler`.
    pub fn withdraw_volatile(
        ctx: Context<WithdrawVolatileAccountConstraints>,
        amount: u64,
    ) -> Result<()> {
        instructions::withdraw_volatile::handler(ctx, amount)
    }
}
