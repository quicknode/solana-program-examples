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

    /// Create a curated whitelist of assets, owned by `authority` (not a manager).
    pub fn initialize_registry(
        context: Context<InitializeRegistryAccountConstraints>,
    ) -> Result<()> {
        instructions::initialize_registry::handle_initialize_registry(context)
    }

    /// Approve a mint and bind it to its official price feed. Registry authority only.
    pub fn whitelist_asset(
        context: Context<WhitelistAssetAccountConstraints>,
        price_feed: Pubkey,
    ) -> Result<()> {
        instructions::whitelist_asset::handle_whitelist_asset(context, price_feed)
    }

    /// Open a strategy at a caller-chosen index, e.g. index 0 derives the PDA
    /// from seeds `"strategy" + 0`. Manager pays and becomes the strategy's manager.
    pub fn initialize_strategy(
        context: Context<InitializeStrategyAccountConstraints>,
        index: u64,
        fee_bps: u16,
        max_slippage_bps: u16,
        swap_router: Pubkey,
    ) -> Result<()> {
        instructions::initialize_strategy::handle_initialize_strategy(
            context,
            index,
            fee_bps,
            max_slippage_bps,
            swap_router,
        )
    }

    /// Add a whitelisted asset to the strategy at the next index. Manager only.
    pub fn add_asset(context: Context<AddAssetAccountConstraints>, weight_bps: u16) -> Result<()> {
        instructions::add_asset::handle_add_asset(context, weight_bps)
    }

    /// Change an asset's target weight, or set it to zero to retire it. Manager only.
    pub fn set_weight(
        context: Context<SetWeightAccountConstraints>,
        weight_bps: u16,
    ) -> Result<()> {
        instructions::set_weight::handle_set_weight(context, weight_bps)
    }

    pub fn deposit<'info>(
        context: Context<'info, DepositAccountConstraints<'info>>,
        usdc_amount: u64,
        minimum_shares: u64,
    ) -> Result<()> {
        instructions::deposit::handle_deposit(context, usdc_amount, minimum_shares)
    }

    pub fn invest(context: Context<InvestAccountConstraints>, usdc_amount: u64) -> Result<()> {
        instructions::invest::handle_invest(context, usdc_amount)
    }

    pub fn collect_fees(context: Context<CollectFeesAccountConstraints>) -> Result<()> {
        instructions::collect_fees::handle_collect_fees(context)
    }

    pub fn withdraw<'info>(
        context: Context<'info, WithdrawAccountConstraints<'info>>,
        shares_to_burn: u64,
        min_usdc_out: u64,
    ) -> Result<()> {
        instructions::withdraw::handle_withdraw(context, shares_to_burn, min_usdc_out)
    }

    pub fn rebalance(
        context: Context<RebalanceAccountConstraints>,
        sell_amount: u64,
        usdc_to_invest: u64,
    ) -> Result<()> {
        instructions::rebalance::handle_rebalance(context, sell_amount, usdc_to_invest)
    }
}
