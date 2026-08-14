use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod instructions;
pub mod math;
pub mod state;

use instructions::*;
use state::ReserveConfig;

declare_id!("4bvT6A8S7ZVL6bSvK2KoL2nQ4F5H6AF9133kCYbMJj1t");

#[program]
pub mod lending {
    use super::*;

    pub fn initialize_lending_market(
        context: &mut Context<InitializeLendingMarket>,
        market_id: u64,
    ) -> Result<()> {
        instructions::handle_initialize_lending_market(context, market_id)
    }

    pub fn initialize_reserve(
        context: &mut Context<InitializeReserve>,
        config: ReserveConfig,
    ) -> Result<()> {
        instructions::handle_initialize_reserve(context, config)
    }

    pub fn update_reserve_config(
        context: &mut Context<UpdateReserveConfig>,
        config: ReserveConfig,
    ) -> Result<()> {
        instructions::handle_update_reserve_config(context, config)
    }

    pub fn collect_protocol_fees(context: &mut Context<CollectProtocolFees>) -> Result<()> {
        instructions::handle_collect_protocol_fees(context)
    }

    pub fn set_price(
        context: &mut Context<SetPrice>,
        price_mantissa: i128,
        exponent: i32,
    ) -> Result<()> {
        instructions::handle_set_price(context, price_mantissa, exponent)
    }

    pub fn refresh_reserve(context: &mut Context<RefreshReserve>) -> Result<()> {
        instructions::handle_refresh_reserve(context)
    }

    pub fn deposit_reserve_liquidity(
        context: &mut Context<DepositReserveLiquidity>,
        liquidity_amount: u64,
    ) -> Result<()> {
        instructions::handle_deposit_reserve_liquidity(context, liquidity_amount)
    }

    pub fn redeem_reserve_collateral(
        context: &mut Context<RedeemReserveCollateral>,
        share_amount: u64,
    ) -> Result<()> {
        instructions::handle_redeem_reserve_collateral(context, share_amount)
    }

    pub fn initialize_obligation(context: &mut Context<InitializeObligation>) -> Result<()> {
        instructions::handle_initialize_obligation(context)
    }

    pub fn refresh_obligation(context: &mut Context<RefreshObligation>) -> Result<()> {
        instructions::handle_refresh_obligation(context)
    }

    pub fn deposit_obligation_collateral(
        context: &mut Context<DepositObligationCollateral>,
        share_amount: u64,
    ) -> Result<()> {
        instructions::handle_deposit_obligation_collateral(context, share_amount)
    }

    pub fn withdraw_obligation_collateral(
        context: &mut Context<WithdrawObligationCollateral>,
        share_amount: u64,
    ) -> Result<()> {
        instructions::handle_withdraw_obligation_collateral(context, share_amount)
    }

    pub fn borrow_obligation_liquidity(
        context: &mut Context<BorrowObligationLiquidity>,
        liquidity_amount: u64,
    ) -> Result<()> {
        instructions::handle_borrow_obligation_liquidity(context, liquidity_amount)
    }

    pub fn repay_obligation_liquidity(
        context: &mut Context<RepayObligationLiquidity>,
        liquidity_amount: u64,
    ) -> Result<()> {
        instructions::handle_repay_obligation_liquidity(context, liquidity_amount)
    }

    pub fn liquidate_obligation(
        context: &mut Context<LiquidateObligation>,
        liquidity_amount: u64,
    ) -> Result<()> {
        instructions::handle_liquidate_obligation(context, liquidity_amount)
    }
}
