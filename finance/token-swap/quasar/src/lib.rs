#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

pub mod error;
pub mod instructions;
use instructions::*;
pub mod liquidity;
pub mod state;
#[cfg(test)]
mod tests;

declare_id!("GahM6PrXesrBkHiGJ5no4EskLNnVBCaSwVKbM4UtzyK6");

/// Minimum liquidity withheld from the creator's deposit in `initialize_pool`
/// to prevent manipulation.
pub const MINIMUM_LIQUIDITY: u64 = 100;
/// Basis-points denominator (1 bp = 1/10_000). Fees and the admin's fee share
/// are stored in basis points; dividing by this converts a bp value to a
/// fraction. Keeps the bare 10_000 out of the math.
pub const BASIS_POINTS_DIVISOR: u64 = 10_000;
/// Seed for the global Config PDA (singleton).
pub const CONFIG_SEED: &[u8] = b"config";
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
///
/// This account is also the pool's signing authority: it owns both reserves,
/// is the LP mint's mint authority, and signs the transfers out of the
/// reserves and the LP mint with these seeds plus its bump.
#[derive(Seeds)]
#[seeds(b"", config: Address, mint_a: Address, mint_b: Address)]
pub struct PoolPda;

/// Liquidity-mint PDA at seeds = [b"liquidity", config, mint_a, mint_b].
#[derive(Seeds)]
#[seeds(b"liquidity", config: Address, mint_a: Address, mint_b: Address)]
pub struct LiquidityMintPda;

/// The pool's token A reserve, a PDA of the pool at seeds = [b"pool_a",
/// pool_config], so any client can derive where a pool keeps its tokens
/// without being told. `PoolConfig` also records the address, and every
/// handler that touches the reserve checks it with `has_one(pool_a)`.
#[derive(Seeds)]
#[seeds(b"pool_a", pool_config: Address)]
pub struct PoolAPda;

/// The pool's token B reserve, at seeds = [b"pool_b", pool_config], recorded
/// and checked as `pool_a` is.
#[derive(Seeds)]
#[seeds(b"pool_b", pool_config: Address)]
pub struct PoolBPda;

/// Simple constant-product AMM (token swap).
///
/// Six instructions:
/// 1. `initialize_config` - initialise the singleton AMM config (admin, fee,
///    admin share)
/// 2. `initialize_pool` - create a liquidity pool for a token pair, funded
///    and priced by its creator's first deposit
/// 3. `deposit_liquidity` - add liquidity at the pool's price and receive LP
///    tokens
/// 4. `withdraw_liquidity` - burn LP tokens and receive pool tokens
/// 5. `swap_tokens` - swap one token for another
/// 6. `claim_admin_fees` - admin sweeps accumulated fee slice from a pool
#[program]
mod quasar_token_swap {
    use super::*;

    #[instruction(discriminator = 0)]
    pub fn initialize_config(
        ctx: Ctx<InitializeConfigAccountConstraints>,
        fee: u16,
        admin_share_bps: u16,
    ) -> Result<(), ProgramError> {
        instructions::handle_initialize_config(&mut ctx.accounts, fee, admin_share_bps)
    }

    #[instruction(discriminator = 1)]
    pub fn initialize_pool(
        ctx: Ctx<InitializePoolAccountConstraints>,
        amount_a: u64,
        amount_b: u64,
    ) -> Result<(), ProgramError> {
        instructions::handle_initialize_pool(&mut ctx.accounts, amount_a, amount_b, &ctx.bumps)
    }

    #[instruction(discriminator = 2)]
    pub fn deposit_liquidity(
        ctx: Ctx<DepositLiquidityAccountConstraints>,
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
        ctx: Ctx<WithdrawLiquidityAccountConstraints>,
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
        ctx: Ctx<SwapTokensAccountConstraints>,
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
        ctx: Ctx<ClaimAdminFeesAccountConstraints>,
    ) -> Result<(), ProgramError> {
        instructions::handle_claim_admin_fees(&mut ctx.accounts, &ctx.bumps)
    }
}
