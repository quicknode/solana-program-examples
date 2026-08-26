use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

use crate::{
    constants::{AUTHORITY_SEED, CONFIG_SEED},
    errors::AmmError,
    state::{Config, PoolConfig},
};

/// Sweep the admin's accumulated trading-fee claims for both sides of a pool
/// into the admin's token accounts.
///
/// During each swap, the admin's slice of the fee accumulates as a virtual
/// claim on the input-side reserve (`pool_config.admin_fees_owed_a` /
/// `admin_fees_owed_b`). This handler transfers those amounts out of the
/// pool reserves into the admin's ATAs and resets the accumulators to zero.
///
/// Authorisation: the `has_one = admin` constraint on `config` plus the
/// `Signer` constraint on `admin` together mean only the address stored in
/// `Config.admin` can call this. Any other signer will be rejected by
/// Anchor's built-in `has_one` check.
pub fn handle_claim_admin_fees(context: Context<ClaimAdminFeesAccountConstraints>) -> Result<()> {
    let owed_a = context.accounts.pool_config.admin_fees_owed_a;
    let owed_b = context.accounts.pool_config.admin_fees_owed_b;

    // Revert if there's nothing to claim. Two reasons:
    //   1. It tells the admin offchain that the call did nothing - silent
    //      no-ops mask wasted txs.
    //   2. Under litesvm, two byte-identical claim txs (same payer, same
    //      accounts, same recent_blockhash) produce the same signature and
    //      the runtime rejects the second as `AlreadyProcessed`. Failing
    //      explicitly here gives callers a real error to handle.
    if owed_a == 0 && owed_b == 0 {
        return err!(AmmError::NothingToClaim);
    }

    // Pre-copy seed bytes before the mutable borrow of pool_config.
    let authority_bump = context.bumps.pool_authority;
    let config_bytes = context.accounts.pool_config.config.to_bytes();
    let mint_a_bytes = context.accounts.mint_a.key().to_bytes();
    let mint_b_bytes = context.accounts.mint_b.key().to_bytes();

    // Effects: zero the accumulators before the CPIs (Checks-Effects-Interactions).
    // If a CPI fails the whole transaction reverts, so the state reset is safe.
    {
        let pool_config = &mut context.accounts.pool_config;
        pool_config.admin_fees_owed_a = 0;
        pool_config.admin_fees_owed_b = 0;
    }

    // Interactions: transfer the owed fees out of the pool reserves.
    let authority_seeds = &[
        config_bytes.as_ref(),
        mint_a_bytes.as_ref(),
        mint_b_bytes.as_ref(),
        AUTHORITY_SEED,
        &[authority_bump],
    ];
    let signer_seeds = &[&authority_seeds[..]];

    if owed_a > 0 {
        token_interface::transfer_checked(
            CpiContext::new_with_signer(
                context.accounts.token_program.key(),
                TransferChecked {
                    from: context.accounts.pool_a.to_account_info(),
                    mint: context.accounts.mint_a.to_account_info(),
                    to: context.accounts.admin_token_a.to_account_info(),
                    authority: context.accounts.pool_authority.to_account_info(),
                },
                signer_seeds,
            ),
            owed_a,
            context.accounts.mint_a.decimals,
        )?;
    }

    if owed_b > 0 {
        token_interface::transfer_checked(
            CpiContext::new_with_signer(
                context.accounts.token_program.key(),
                TransferChecked {
                    from: context.accounts.pool_b.to_account_info(),
                    mint: context.accounts.mint_b.to_account_info(),
                    to: context.accounts.admin_token_b.to_account_info(),
                    authority: context.accounts.pool_authority.to_account_info(),
                },
                signer_seeds,
            ),
            owed_b,
            context.accounts.mint_b.decimals,
        )?;
    }

    msg!("Admin swept fees: {} of mint_a, {} of mint_b", owed_a, owed_b);

    Ok(())
}

#[derive(Accounts)]
pub struct ClaimAdminFeesAccountConstraints<'info> {
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = admin,
    )]
    pub config: Account<'info, Config>,

    #[account(
        mut,
        seeds = [
            pool_config.config.as_ref(),
            pool_config.mint_a.key().as_ref(),
            pool_config.mint_b.key().as_ref(),
        ],
        bump = pool_config.bump,
        has_one = config,
        has_one = mint_a,
        has_one = mint_b,
    )]
    pub pool_config: Account<'info, PoolConfig>,

    /// CHECK: PDA that owns the pool reserves; signs the outbound transfers.
    #[account(
        seeds = [
            pool_config.config.as_ref(),
            mint_a.key().as_ref(),
            mint_b.key().as_ref(),
            AUTHORITY_SEED,
        ],
        bump,
    )]
    pub pool_authority: UncheckedAccount<'info>,

    pub mint_a: Box<InterfaceAccount<'info, Mint>>,

    pub mint_b: Box<InterfaceAccount<'info, Mint>>,

    /// The pool's token-A reserve. The admin's owed token-A fees are paid out
    /// of this account.
    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = pool_authority,
        associated_token::token_program = token_program,
    )]
    pub pool_a: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The pool's token-B reserve. The admin's owed token-B fees are paid out
    /// of this account.
    #[account(
        mut,
        associated_token::mint = mint_b,
        associated_token::authority = pool_authority,
        associated_token::token_program = token_program,
    )]
    pub pool_b: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Must match the address stored in `Config.admin` (enforced by
    /// `has_one = admin` above).
    pub admin: Signer<'info>,

    /// Admin's token-A receiving account. Must already exist; the admin is
    /// expected to create it themselves before calling. Keeps this handler
    /// small (no `init_if_needed`).
    #[account(
        mut,
        token::mint = mint_a,
        token::authority = admin,
        token::token_program = token_program,
    )]
    pub admin_token_a: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Admin's token-B receiving account. Same constraints as `admin_token_a`.
    #[account(
        mut,
        token::mint = mint_b,
        token::authority = admin,
        token::token_program = token_program,
    )]
    pub admin_token_b: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_program: Interface<'info, TokenInterface>,
}
