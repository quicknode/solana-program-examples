#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

pub mod errors;
pub mod instructions;
pub mod oracle;
pub mod state;

use instructions::*;

#[cfg(test)]
mod tests;

declare_id!("VLT5W7bqhRN4nCdRpXm8UfHRxZd9EuZGqiSAkGHQfGh");

/// Tokenized multi-asset vault strategy. A manager assembles a basket of
/// curator-approved assets at target weights; depositors receive shares priced at
/// net asset value, and each deposit is immediately deployed into the basket by
/// swapping USDC through a registered router. Withdrawals redeem shares for a
/// proportional slice of every vault. See README.md for the full walkthrough.
#[program]
mod quasar_vault_strategy {
    use super::*;

    /// Create a curated approved_asset of assets, owned by `authority` (not a
    /// manager).
    #[instruction(discriminator = 0)]
    pub fn initialize_registry(
        ctx: Ctx<InitializeRegistryAccountConstraints>,
    ) -> Result<(), ProgramError> {
        instructions::initialize_registry::handle_initialize_registry(&mut ctx.accounts, &ctx.bumps)
    }

    /// Approve a mint and bind it to its official price feed. Registry authority
    /// only.
    #[instruction(discriminator = 1)]
    pub fn approve_asset(
        ctx: Ctx<ApproveAssetAccountConstraints>,
        price_feed: Address,
    ) -> Result<(), ProgramError> {
        instructions::approve_asset::handle_approve_asset(&mut ctx.accounts, price_feed, &ctx.bumps)
    }

    /// Open a strategy at a caller-chosen index. Manager pays and becomes the
    /// strategy's manager.
    #[instruction(discriminator = 2)]
    pub fn initialize_strategy(
        ctx: Ctx<InitializeStrategyAccountConstraints>,
        index: u64,
        fee_bps: u16,
        max_slippage_bps: u16,
        swap_router: Address,
    ) -> Result<(), ProgramError> {
        instructions::initialize_strategy::handle_initialize_strategy(
            &mut ctx.accounts,
            index,
            fee_bps,
            max_slippage_bps,
            swap_router,
            &ctx.bumps,
        )
    }

    /// Add a curator-approved asset to the strategy at the next index. Manager only.
    #[instruction(discriminator = 3)]
    pub fn add_asset(
        ctx: Ctx<AddAssetAccountConstraints>,
        weight_bps: u16,
    ) -> Result<(), ProgramError> {
        instructions::add_asset::handle_add_asset(&mut ctx.accounts, weight_bps, &ctx.bumps)
    }

    /// Change an asset's target weight, or set it to zero to retire it. Manager
    /// only.
    #[instruction(discriminator = 4)]
    pub fn set_weight(
        ctx: Ctx<SetWeightAccountConstraints>,
        weight_bps: u16,
    ) -> Result<(), ProgramError> {
        instructions::set_weight::handle_set_weight(&mut ctx.accounts, weight_bps)
    }

    /// Deposit USDC, receive shares priced at net asset value, and immediately
    /// deploy the deposit into the basket at its target weights.
    #[instruction(discriminator = 5)]
    pub fn deposit(
        ctx: CtxWithRemaining<DepositAccountConstraints>,
        usdc_amount: u64,
        minimum_shares: u64,
    ) -> Result<(), ProgramError> {
        let remaining = ctx.remaining_accounts();
        instructions::deposit::handle_deposit(
            &mut ctx.accounts,
            remaining,
            usdc_amount,
            minimum_shares,
        )
    }

    /// Accrue the time-based management fee, minting fresh shares to the manager.
    #[instruction(discriminator = 6)]
    pub fn collect_fees(ctx: Ctx<CollectFeesAccountConstraints>) -> Result<(), ProgramError> {
        instructions::collect_fees::handle_collect_fees(&mut ctx.accounts)
    }

    /// Burn shares and redeem a proportional slice of the USDC vault and every
    /// asset vault, paid in kind.
    #[instruction(discriminator = 7)]
    pub fn withdraw(
        ctx: CtxWithRemaining<WithdrawAccountConstraints>,
        shares_to_burn: u64,
        min_usdc_out: u64,
    ) -> Result<(), ProgramError> {
        let remaining = ctx.remaining_accounts();
        instructions::withdraw::handle_withdraw(
            &mut ctx.accounts,
            remaining,
            shares_to_burn,
            min_usdc_out,
        )
    }

    /// Sell one basket asset for USDC and buy another with it, keeping the
    /// basket near its target weights. Manager only.
    #[instruction(discriminator = 8)]
    pub fn rebalance(
        ctx: Ctx<RebalanceAccountConstraints>,
        sell_amount: u64,
        usdc_to_invest: u64,
    ) -> Result<(), ProgramError> {
        instructions::rebalance::handle_rebalance(&mut ctx.accounts, sell_amount, usdc_to_invest)
    }
}
