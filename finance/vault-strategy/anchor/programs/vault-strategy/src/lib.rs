pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use instructions::*;
pub use state::*;

declare_id!("VLT5W7bqhRN4nCdRpXm8UfHRxZd9EuZGqiSAkGHQfGh");

#[program]
pub mod vault_strategy {
    use super::*;

    pub fn initialize_strategy(
        context: Context<InitializeStrategyAccountConstraints>,
        weight_bps_a: u16,
        weight_bps_b: u16,
        fee_bps: u16,
        swap_router: Pubkey,
        price_feed_a: Pubkey,
        price_feed_b: Pubkey,
    ) -> Result<()> {
        instructions::initialize_strategy::handle_initialize_strategy(
            context,
            weight_bps_a,
            weight_bps_b,
            fee_bps,
            swap_router,
            price_feed_a,
            price_feed_b,
        )
    }

    pub fn deposit(
        context: Context<DepositAccountConstraints>,
        usdc_amount: u64,
        minimum_shares: u64,
    ) -> Result<()> {
        instructions::deposit::handle_deposit(context, usdc_amount, minimum_shares)
    }

    pub fn invest(
        context: Context<InvestAccountConstraints>,
        usdc_amount: u64,
        minimum_asset_out: u64,
    ) -> Result<()> {
        instructions::invest::handle_invest(context, usdc_amount, minimum_asset_out)
    }

    pub fn collect_fees(context: Context<CollectFeesAccountConstraints>) -> Result<()> {
        instructions::collect_fees::handle_collect_fees(context)
    }

    pub fn withdraw(
        context: Context<WithdrawAccountConstraints>,
        shares_to_burn: u64,
        min_usdc_out: u64,
        min_asset_a_out: u64,
        min_asset_b_out: u64,
    ) -> Result<()> {
        instructions::withdraw::handle_withdraw(
            context,
            shares_to_burn,
            min_usdc_out,
            min_asset_a_out,
            min_asset_b_out,
        )
    }

    pub fn rebalance(
        context: Context<RebalanceAccountConstraints>,
        sell_amount: u64,
        minimum_usdc_from_sell: u64,
        usdc_to_invest: u64,
        minimum_buy_amount: u64,
    ) -> Result<()> {
        instructions::rebalance::handle_rebalance(
            context,
            sell_amount,
            minimum_usdc_from_sell,
            usdc_to_invest,
            minimum_buy_amount,
        )
    }
}
