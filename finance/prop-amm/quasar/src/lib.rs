#![cfg_attr(not(test), no_std)]

//! Quasar port of the prop-amm example. The design, math, and behaviour match
//! the Anchor sibling at `finance/prop-amm/anchor`; see its README for the
//! full walkthrough. This file wires up the program; the per-instruction logic
//! lives in `instructions/`.

use quasar_lang::prelude::*;

mod constants;
mod instructions;
mod last_restart;
pub mod state;
#[cfg(test)]
mod tests;

use instructions::*;

declare_id!("FPTx81bSwghfrwzaQpgmKPw1TnajK66wif1cQyev4GdD");

/// Authority PDA at seeds = [b"authority", market]. Signs vault CPIs.
#[derive(Seeds)]
#[seeds(b"authority", market: Address)]
pub struct MarketAuthorityPda;

/// Base-token vault PDA at seeds = [b"base_vault", market].
#[derive(Seeds)]
#[seeds(b"base_vault", market: Address)]
pub struct BaseVaultPda;

/// Quote-token vault PDA at seeds = [b"quote_vault", market].
#[derive(Seeds)]
#[seeds(b"quote_vault", market: Address)]
pub struct QuoteVaultPda;

#[program]
mod quasar_prop_amm {
    use super::*;

    #[instruction(discriminator = 0)]
    pub fn initialize_market(
        ctx: Ctx<InitializeMarket>,
        oracle_scale: u32,
        spread_bps: u16,
        max_confidence_bps: u16,
    ) -> Result<(), ProgramError> {
        instructions::handle_initialize_market(
            &mut ctx.accounts,
            oracle_scale,
            spread_bps,
            max_confidence_bps,
            &ctx.bumps,
        )
    }

    #[instruction(discriminator = 1)]
    pub fn deposit_inventory(
        ctx: Ctx<DepositInventory>,
        base_amount: u64,
        quote_amount: u64,
    ) -> Result<(), ProgramError> {
        instructions::handle_deposit_inventory(&mut ctx.accounts, base_amount, quote_amount)
    }

    #[instruction(discriminator = 2)]
    pub fn withdraw_inventory(
        ctx: Ctx<WithdrawInventory>,
        base_amount: u64,
        quote_amount: u64,
    ) -> Result<(), ProgramError> {
        instructions::handle_withdraw_inventory(&mut ctx.accounts, base_amount, quote_amount)
    }

    #[instruction(discriminator = 3)]
    pub fn set_quote(
        ctx: Ctx<SetQuote>,
        spread_bps: u16,
        paused: u8,
    ) -> Result<(), ProgramError> {
        instructions::handle_set_quote(&mut ctx.accounts, spread_bps, paused)
    }

    #[instruction(discriminator = 4)]
    pub fn swap(
        ctx: Ctx<Swap>,
        direction: u8,
        amount_in: u64,
        minimum_amount_out: u64,
    ) -> Result<(), ProgramError> {
        instructions::handle_swap(&mut ctx.accounts, direction, amount_in, minimum_amount_out)
    }
}
