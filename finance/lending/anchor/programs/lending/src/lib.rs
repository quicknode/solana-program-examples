use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod math;
pub mod instructions;
pub mod state;

use instructions::*;
use state::ReserveConfig;

declare_id!("4bvT6A8S7ZVL6bSvK2KoL2nQ4F5H6AF9133kCYbMJj1t");

#[program]
pub mod lending {
    use super::*;

    pub fn init_lending_market(context: Context<InitLendingMarket>) -> Result<()> {
        instructions::handle_init_lending_market(context)
    }

    pub fn init_reserve(context: Context<InitReserve>, config: ReserveConfig) -> Result<()> {
        instructions::handle_init_reserve(context, config)
    }

    pub fn update_reserve_config(
        context: Context<UpdateReserveConfig>,
        config: ReserveConfig,
    ) -> Result<()> {
        instructions::handle_update_reserve_config(context, config)
    }

    pub fn collect_protocol_fees(context: Context<CollectProtocolFees>) -> Result<()> {
        instructions::handle_collect_protocol_fees(context)
    }

    pub fn set_price(
        context: Context<SetPrice>,
        price_mantissa: i128,
        exponent: i32,
    ) -> Result<()> {
        instructions::handle_set_price(context, price_mantissa, exponent)
    }

    pub fn refresh_reserve(context: Context<RefreshReserve>) -> Result<()> {
        instructions::handle_refresh_reserve(context)
    }

    pub fn deposit_reserve_liquidity(
        context: Context<DepositReserveLiquidity>,
        liquidity_amount: u64,
    ) -> Result<()> {
        instructions::handle_deposit_reserve_liquidity(context, liquidity_amount)
    }

    pub fn redeem_reserve_collateral(
        context: Context<RedeemReserveCollateral>,
        share_amount: u64,
    ) -> Result<()> {
        instructions::handle_redeem_reserve_collateral(context, share_amount)
    }

    pub fn init_obligation(context: Context<InitObligation>) -> Result<()> {
        instructions::handle_init_obligation(context)
    }

    pub fn refresh_obligation(context: Context<RefreshObligation>) -> Result<()> {
        instructions::handle_refresh_obligation(context)
    }

    pub fn deposit_obligation_collateral(
        context: Context<DepositObligationCollateral>,
        share_amount: u64,
    ) -> Result<()> {
        instructions::handle_deposit_obligation_collateral(context, share_amount)
    }

    pub fn withdraw_obligation_collateral(
        context: Context<WithdrawObligationCollateral>,
        share_amount: u64,
    ) -> Result<()> {
        instructions::handle_withdraw_obligation_collateral(context, share_amount)
    }

    pub fn borrow_obligation_liquidity(
        context: Context<BorrowObligationLiquidity>,
        liquidity_amount: u64,
    ) -> Result<()> {
        instructions::handle_borrow_obligation_liquidity(context, liquidity_amount)
    }

    pub fn repay_obligation_liquidity(
        context: Context<RepayObligationLiquidity>,
        liquidity_amount: u64,
    ) -> Result<()> {
        instructions::handle_repay_obligation_liquidity(context, liquidity_amount)
    }

    pub fn liquidate_obligation(
        context: Context<LiquidateObligation>,
        liquidity_amount: u64,
    ) -> Result<()> {
        instructions::handle_liquidate_obligation(context, liquidity_amount)
    }
}
