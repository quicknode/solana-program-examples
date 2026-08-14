pub mod error;
pub mod instructions;
pub mod oracle;
pub mod state;

use anchor_lang::prelude::*;

pub use instructions::*;
pub use state::*;

declare_id!("VLT5W7bqhRN4nCdRpXm8UfHRxZd9EuZGqiSAkGHQfGh");

#[program]
pub mod vault_strategy {
    use super::*;

    /// Create the curator record for an approved-asset set, owned by `authority`
    /// (not a manager). The set itself lives in per-asset ApprovedAsset accounts.
    pub fn initialize_registry(
        context: &mut Context<InitializeRegistryAccountConstraints>,
    ) -> Result<()> {
        instructions::initialize_registry::handle_initialize_registry(context)
    }

    /// Approve a mint and bind it to its official price feed. Registry authority only.
    pub fn approve_asset(
        context: &mut Context<ApproveAssetAccountConstraints>,
        price_feed: Address,
    ) -> Result<()> {
        instructions::approve_asset::handle_approve_asset(context, price_feed)
    }

    /// Open a strategy at a caller-chosen index, e.g. index 0 derives the PDA
    /// from seeds `"strategy" + 0`. Manager pays and becomes the strategy's manager.
    pub fn initialize_strategy(
        context: &mut Context<InitializeStrategyAccountConstraints>,
        index: u64,
        fee_bps: u16,
        max_slippage_bps: u16,
        swap_router: Address,
    ) -> Result<()> {
        instructions::initialize_strategy::handle_initialize_strategy(
            context,
            index,
            fee_bps,
            max_slippage_bps,
            swap_router,
        )
    }

    /// Add a curator-approved asset to the strategy at the next index. Manager only.
    pub fn add_asset(
        context: &mut Context<AddAssetAccountConstraints>,
        weight_bps: u16,
    ) -> Result<()> {
        instructions::add_asset::handle_add_asset(context, weight_bps)
    }

    /// Change an asset's target weight, or set it to zero to retire it. Manager only.
    pub fn set_weight(
        context: &mut Context<SetWeightAccountConstraints>,
        weight_bps: u16,
    ) -> Result<()> {
        instructions::set_weight::handle_set_weight(context, weight_bps)
    }

    pub fn deposit<'info>(
        context: &mut Context<'info, DepositAccountConstraints<'info>>,
        usdc_amount: u64,
        minimum_shares: u64,
    ) -> Result<()> {
        instructions::deposit::handle_deposit(context, usdc_amount, minimum_shares)
    }

    pub fn collect_fees(context: &mut Context<CollectFeesAccountConstraints>) -> Result<()> {
        instructions::collect_fees::handle_collect_fees(context)
    }

    pub fn withdraw<'info>(
        context: &mut Context<'info, WithdrawAccountConstraints<'info>>,
        shares_to_burn: u64,
        min_usdc_out: u64,
    ) -> Result<()> {
        instructions::withdraw::handle_withdraw(context, shares_to_burn, min_usdc_out)
    }

    pub fn rebalance(
        context: &mut Context<RebalanceAccountConstraints>,
        sell_amount: u64,
        usdc_to_invest: u64,
    ) -> Result<()> {
        instructions::rebalance::handle_rebalance(context, sell_amount, usdc_to_invest)
    }
}
