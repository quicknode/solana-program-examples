use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::constants::DEFAULT_CRANK_INTERVAL_SECONDS;
use crate::state::Vault;

/// Create a new vault for `owner` and the (volatile, stable) pair.
///
/// The vault account is a PDA at `[b"vault", owner.key().as_ref()]`. Its
/// associated token accounts for the volatile and stable mints are created
/// here so deposit and conversion can run with one fewer instruction.
///
/// TukTuk task registration is intentionally stubbed — see the inline TODO.
/// The owner can do it offchain too; the vault just records the resulting
/// task pubkey for discoverability.
pub fn handler(
    ctx: Context<InitializeVaultAccountConstraints>,
    threshold_price: i128,
    crank_interval_seconds: u32,
    tuktuk_task: Pubkey,
) -> Result<()> {
    let vault = &mut ctx.accounts.vault;
    vault.owner = ctx.accounts.owner.key();
    vault.volatile_mint = ctx.accounts.volatile_mint.key();
    vault.stable_mint = ctx.accounts.stable_mint.key();
    vault.oracle_feed = ctx.accounts.oracle_feed.key();
    vault.threshold_price = threshold_price;
    vault.crank_interval_seconds = if crank_interval_seconds == 0 {
        DEFAULT_CRANK_INTERVAL_SECONDS
    } else {
        crank_interval_seconds
    };
    vault.tuktuk_task = tuktuk_task;
    vault.triggered = false;
    vault.bump = ctx.bumps.vault;

    // TODO: TukTuk task registration — see github.com/helium/tuktuk for the
    // real CPI. The production version of this handler should CPI into
    // TukTuk's `task_init` here so the task is created atomically with the
    // vault. For this teaching example, we accept the `tuktuk_task` pubkey as
    // an input the owner has pre-created (or zeroed out for tests that don't
    // exercise the scheduler).
    Ok(())
}

#[derive(Accounts)]
pub struct InitializeVaultAccountConstraints<'info> {
    #[account(
        init,
        payer = owner,
        space = Vault::DISCRIMINATOR.len() + Vault::INIT_SPACE,
        seeds = [Vault::SEED_PREFIX, owner.key().as_ref()],
        bump,
    )]
    pub vault: Account<'info, Vault>,

    pub volatile_mint: InterfaceAccount<'info, Mint>,

    pub stable_mint: InterfaceAccount<'info, Mint>,

    /// CHECK: arbitrary Switchboard feed pubkey; layout is verified at read
    /// time inside `convert_if_triggered`.
    pub oracle_feed: UncheckedAccount<'info>,

    #[account(
        init,
        payer = owner,
        associated_token::mint = volatile_mint,
        associated_token::authority = vault,
    )]
    pub vault_volatile_account: InterfaceAccount<'info, TokenAccount>,

    #[account(
        init,
        payer = owner,
        associated_token::mint = stable_mint,
        associated_token::authority = vault,
    )]
    pub vault_stable_account: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub owner: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}
