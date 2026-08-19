use anchor_lang::prelude::*;

mod constants;
pub mod errors;
pub mod instructions;
mod state;

// The `#[derive(Accounts)]` client modules are generated beside their structs,
// and `#[program]` resolves them at `super::`, the crate root. Re-exporting
// here rather than inside the module puts them where it looks.
use instructions::*;

declare_id!("GahM6PrXesrBkHiGJ5no4EskLNnVBCaSwVKbM4UtzyK6");

#[program]
pub mod swap_example {
    use super::*;

    pub fn initialize_config(
        context: &mut Context<InitializeConfigAccountConstraints>,
        fee: u16,
        admin_share_bps: u16,
    ) -> Result<()> {
        instructions::handle_initialize_config(context, fee, admin_share_bps)
    }

    pub fn initialize_pool(context: &mut Context<InitializePoolAccountConstraints>) -> Result<()> {
        instructions::handle_initialize_pool(context)
    }

    pub fn deposit_liquidity(
        context: &mut Context<DepositLiquidityAccountConstraints>,
        amount_a: u64,
        amount_b: u64,
        minimum_lp_tokens_out: u64,
    ) -> Result<()> {
        instructions::handle_deposit_liquidity(context, amount_a, amount_b, minimum_lp_tokens_out)
    }

    pub fn withdraw_liquidity(
        context: &mut Context<WithdrawLiquidityAccountConstraints>,
        amount: u64,
        minimum_token_a_out: u64,
        minimum_token_b_out: u64,
    ) -> Result<()> {
        instructions::handle_withdraw_liquidity(
            context,
            amount,
            minimum_token_a_out,
            minimum_token_b_out,
        )
    }

    pub fn swap_tokens(
        context: &mut Context<SwapTokensAccountConstraints>,
        input_is_token_a: bool,
        input_amount: u64,
        min_output_amount: u64,
    ) -> Result<()> {
        instructions::handle_swap_tokens(context, input_is_token_a, input_amount, min_output_amount)
    }

    pub fn claim_admin_fees(context: &mut Context<ClaimAdminFeesAccountConstraints>) -> Result<()> {
        instructions::handle_claim_admin_fees(context)
    }
}
