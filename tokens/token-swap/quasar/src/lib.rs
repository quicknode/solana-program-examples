#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

mod instructions;
use instructions::*;
pub mod state;
#[cfg(test)]
mod tests;

declare_id!("22222222222222222222222222222222222222222222");

/// Minimum liquidity locked on first deposit to prevent manipulation.
pub const MINIMUM_LIQUIDITY: u64 = 100;
/// Seed for the pool authority PDA.
pub const AUTHORITY_SEED: &[u8] = b"authority";
/// Seed for the liquidity mint PDA.
pub const LIQUIDITY_SEED: &[u8] = b"liquidity";

// PDA seed markers required since PR #195 (inline `seeds = [...]` is gone).
// Each marker captures the prefix and Address args; `address = T::seeds(...)`
// drives derivation in the `#[account]` constraint.

/// AMM PDA at seeds = [b"amm"].
#[derive(Seeds)]
#[seeds(b"amm")]
pub struct AmmPda;

/// Pool PDA at seeds = [amm, mint_a, mint_b] — no string prefix.
#[derive(Seeds)]
#[seeds(b"", amm: Address, mint_a: Address, mint_b: Address)]
pub struct PoolPda;

/// Pool-authority PDA at seeds = [amm, mint_a, mint_b, b"authority"].
/// Modelled with prefix b"authority" + the three Address args; the
/// rendered slice list ends up [amm, mint_a, mint_b, b"authority"] when
/// you use `with_bump`. Note: the new \`#[seeds]\` puts the literal
/// prefix first, so the on-chain derivation order is
/// [b"authority", amm, mint_a, mint_b] — different from the original
/// Anchor scheme. Programs are independent so this is consistent and
/// correct on its own; the addresses just won't match the Anchor copy.
#[derive(Seeds)]
#[seeds(b"authority", amm: Address, mint_a: Address, mint_b: Address)]
pub struct PoolAuthorityPda;

/// Liquidity-mint PDA at seeds = [b"liquidity", amm, mint_a, mint_b].
#[derive(Seeds)]
#[seeds(b"liquidity", amm: Address, mint_a: Address, mint_b: Address)]
pub struct LiquidityMintPda;

/// Simple constant-product AMM (token swap).
///
/// Five instructions:
/// 1. `create_amm` — register a new AMM with admin + fee
/// 2. `create_pool` — create a liquidity pool for a token pair
/// 3. `deposit_liquidity` — add liquidity and receive LP tokens
/// 4. `withdraw_liquidity` — burn LP tokens and receive pool tokens
/// 5. `swap_exact_tokens_for_tokens` — swap one token for another
#[program]
mod quasar_token_swap {
    use super::*;

    #[instruction(discriminator = 0)]
    pub fn create_amm(
        ctx: Ctx<CreateAmm>,
        id: Address,
        fee: u16,
    ) -> Result<(), ProgramError> {
        instructions::handle_create_amm(&mut ctx.accounts, id, fee)
    }

    #[instruction(discriminator = 1)]
    pub fn create_pool(ctx: Ctx<CreatePool>) -> Result<(), ProgramError> {
        instructions::handle_create_pool(&mut ctx.accounts)
    }

    #[instruction(discriminator = 2)]
    pub fn deposit_liquidity(
        ctx: Ctx<DepositLiquidity>,
        amount_a: u64,
        amount_b: u64,
    ) -> Result<(), ProgramError> {
        instructions::handle_deposit_liquidity(&mut ctx.accounts, amount_a, amount_b, &ctx.bumps)
    }

    #[instruction(discriminator = 3)]
    pub fn withdraw_liquidity(
        ctx: Ctx<WithdrawLiquidity>,
        amount: u64,
    ) -> Result<(), ProgramError> {
        instructions::handle_withdraw_liquidity(&mut ctx.accounts, amount, &ctx.bumps)
    }

    #[instruction(discriminator = 4)]
    pub fn swap_exact_tokens_for_tokens(
        ctx: Ctx<SwapExactTokensForTokens>,
        swap_a: bool,
        input_amount: u64,
        min_output_amount: u64,
    ) -> Result<(), ProgramError> {
        instructions::handle_swap_exact_tokens_for_tokens(
            &mut ctx.accounts,
            swap_a,
            input_amount,
            min_output_amount,
            &ctx.bumps,
        )
    }
}
