#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

pub mod error;
mod instructions;
use instructions::*;
pub mod state;
#[cfg(test)]
mod tests;

declare_id!("GahM6PrXesrBkHiGJ5no4EskLNnVBCaSwVKbM4UtzyK6");

/// Minimum liquidity locked on first deposit to prevent manipulation.
pub const MINIMUM_LIQUIDITY: u64 = 100;
/// Basis-points denominator (1 bp = 1/10_000). Fees and the admin's fee share
/// are stored in basis points; dividing by this converts a bp value to a
/// fraction. Keeps the bare 10_000 out of the math.
pub const BASIS_POINTS_DIVISOR: u64 = 10_000;
/// Seed for the global Config PDA (singleton).
pub const CONFIG_SEED: &[u8] = b"config";
/// Seed for the pool authority PDA.
pub const AUTHORITY_SEED: &[u8] = b"authority";
/// Seed for the liquidity mint PDA.
pub const LIQUIDITY_SEED: &[u8] = b"liquidity";

// PDA seed markers required since PR #195 (inline `seeds = [...]` is gone).
// Each marker captures the prefix and Address args; `address = T::seeds(...)`
// drives derivation in the `#[account]` constraint.

/// Singleton `Config` PDA at seeds = [b"config"]. One per deployed program.
#[derive(Seeds)]
#[seeds(b"config")]
pub struct ConfigPda;

/// `PoolConfig` PDA at seeds = [config, mint_a, mint_b] - no string prefix.
#[derive(Seeds)]
#[seeds(b"", config: Address, mint_a: Address, mint_b: Address)]
pub struct PoolPda;

/// Pool-authority PDA at seeds = [config, mint_a, mint_b, b"authority"].
/// Modelled with prefix b"authority" + the three Address args; the
/// rendered slice list ends up [config, mint_a, mint_b, b"authority"] when
/// you use `with_bump`. Note: the new \`#[seeds]\` puts the literal
/// prefix first, so the onchain derivation order is
/// [b"authority", config, mint_a, mint_b] - different from the original
/// Anchor scheme. Programs are independent so this is consistent and
/// correct on its own; the addresses just won't match the Anchor copy.
#[derive(Seeds)]
#[seeds(b"authority", config: Address, mint_a: Address, mint_b: Address)]
pub struct PoolAuthorityPda;

/// Liquidity-mint PDA at seeds = [b"liquidity", config, mint_a, mint_b].
#[derive(Seeds)]
#[seeds(b"liquidity", config: Address, mint_a: Address, mint_b: Address)]
pub struct LiquidityMintPda;

/// Simple constant-product AMM (token swap).
///
/// Six instructions:
/// 1. `create_config` - initialise the singleton AMM config (admin, fee,
///    admin share)
/// 2. `create_pool` - create a liquidity pool for a token pair
/// 3. `deposit_liquidity` - add liquidity and receive LP tokens
/// 4. `withdraw_liquidity` - burn LP tokens and receive pool tokens
/// 5. `swap_tokens` - swap one token for another
/// 6. `claim_admin_fees` - admin sweeps accumulated fee slice from a pool
#[program]
mod quasar_token_swap {
    use super::*;

    #[instruction(discriminator = 0)]
    pub fn create_config(
        ctx: Ctx<CreateConfigAccounts>,
        fee: u16,
        admin_share_bps: u16,
    ) -> Result<(), ProgramError> {
        instructions::handle_create_config(&mut ctx.accounts, fee, admin_share_bps)
    }

    #[instruction(discriminator = 1)]
    pub fn create_pool(ctx: Ctx<CreatePoolAccounts>) -> Result<(), ProgramError> {
        instructions::handle_create_pool(&mut ctx.accounts)
    }

    #[instruction(discriminator = 2)]
    pub fn deposit_liquidity(
        ctx: Ctx<DepositLiquidityAccounts>,
        amount_a: u64,
        amount_b: u64,
        minimum_lp_tokens_out: u64,
    ) -> Result<(), ProgramError> {
        instructions::handle_deposit_liquidity(
            &mut ctx.accounts,
            amount_a,
            amount_b,
            minimum_lp_tokens_out,
            &ctx.bumps,
        )
    }

    #[instruction(discriminator = 3)]
    pub fn withdraw_liquidity(
        ctx: Ctx<WithdrawLiquidityAccounts>,
        amount: u64,
        minimum_token_a_out: u64,
        minimum_token_b_out: u64,
    ) -> Result<(), ProgramError> {
        instructions::handle_withdraw_liquidity(
            &mut ctx.accounts,
            amount,
            minimum_token_a_out,
            minimum_token_b_out,
            &ctx.bumps,
        )
    }

    #[instruction(discriminator = 4)]
    pub fn swap_tokens(
        ctx: Ctx<SwapTokensAccounts>,
        input_is_token_a: bool,
        input_amount: u64,
        min_output_amount: u64,
    ) -> Result<(), ProgramError> {
        instructions::handle_swap_tokens(
            &mut ctx.accounts,
            input_is_token_a,
            input_amount,
            min_output_amount,
            &ctx.bumps,
        )
    }

    #[instruction(discriminator = 5)]
    pub fn claim_admin_fees(
        ctx: Ctx<ClaimAdminFeesAccounts>,
    ) -> Result<(), ProgramError> {
        instructions::handle_claim_admin_fees(&mut ctx.accounts, &ctx.bumps)
    }
}
