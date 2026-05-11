use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount, TransferChecked},
};
use fixed::types::I64F64;

use crate::{
    constants::AUTHORITY_SEED,
    errors::*,
    state::{Amm, Pool},
};

pub fn handle_swap_exact_tokens_for_tokens(
    context: Context<SwapExactTokensForTokens>,
    swap_a: bool,
    input_amount: u64,
    min_output_amount: u64,
) -> Result<()> {
    // Fail fast if the trader lacks the requested input balance. Previously this
    // silently clamped to the available balance, which broke slippage protection
    // for callers - their min_output_amount is computed against the requested
    // input, not the clamped one, so the trade could succeed with worse terms
    // than expected.
    if swap_a && input_amount > context.accounts.trader_account_a.amount {
        return err!(TutorialError::InsufficientBalance);
    }
    if !swap_a && input_amount > context.accounts.trader_account_b.amount {
        return err!(TutorialError::InsufficientBalance);
    }
    let input = input_amount;

    // Apply trading fee, used to compute the output
    let amm = &context.accounts.amm;
    let taxed_input = input - input * amm.fee as u64 / 10000;

    let pool_a = &context.accounts.pool_account_a;
    let pool_b = &context.accounts.pool_account_b;
    let output = if swap_a {
        I64F64::from_num(taxed_input)
            .checked_mul(I64F64::from_num(pool_b.amount))
            .unwrap()
            .checked_div(
                I64F64::from_num(pool_a.amount)
                    .checked_add(I64F64::from_num(taxed_input))
                    .unwrap(),
            )
            .unwrap()
    } else {
        I64F64::from_num(taxed_input)
            .checked_mul(I64F64::from_num(pool_a.amount))
            .unwrap()
            .checked_div(
                I64F64::from_num(pool_b.amount)
                    .checked_add(I64F64::from_num(taxed_input))
                    .unwrap(),
            )
            .unwrap()
    }
    .to_num::<u64>();

    if output < min_output_amount {
        return err!(TutorialError::OutputTooSmall);
    }

    // Compute the invariant before the trade
    let invariant = pool_a.amount * pool_b.amount;

    // Transfer tokens to the pool
    let authority_bump = context.bumps.pool_authority;
    let authority_seeds = &[
        &context.accounts.pool.amm.to_bytes(),
        &context.accounts.mint_a.key().to_bytes(),
        &context.accounts.mint_b.key().to_bytes(),
        AUTHORITY_SEED,
        &[authority_bump],
    ];
    let signer_seeds = &[&authority_seeds[..]];
    // Use transfer_checked so the mint + decimals are verified at the token
    // program. This protects callers from decimal-mismatch bugs and is the
    // modern recommended path.
    if swap_a {
        token::transfer_checked(
            CpiContext::new(
                context.accounts.token_program.key(),
                TransferChecked {
                    from: context.accounts.trader_account_a.to_account_info(),
                    mint: context.accounts.mint_a.to_account_info(),
                    to: context.accounts.pool_account_a.to_account_info(),
                    authority: context.accounts.trader.to_account_info(),
                },
            ),
            input,
            context.accounts.mint_a.decimals,
        )?;
        token::transfer_checked(
            CpiContext::new_with_signer(
                context.accounts.token_program.key(),
                TransferChecked {
                    from: context.accounts.pool_account_b.to_account_info(),
                    mint: context.accounts.mint_b.to_account_info(),
                    to: context.accounts.trader_account_b.to_account_info(),
                    authority: context.accounts.pool_authority.to_account_info(),
                },
                signer_seeds,
            ),
            output,
            context.accounts.mint_b.decimals,
        )?;
    } else {
        token::transfer_checked(
            CpiContext::new_with_signer(
                context.accounts.token_program.key(),
                TransferChecked {
                    from: context.accounts.pool_account_a.to_account_info(),
                    mint: context.accounts.mint_a.to_account_info(),
                    to: context.accounts.trader_account_a.to_account_info(),
                    authority: context.accounts.pool_authority.to_account_info(),
                },
                signer_seeds,
            ),
            input,
            context.accounts.mint_a.decimals,
        )?;
        token::transfer_checked(
            CpiContext::new(
                context.accounts.token_program.key(),
                TransferChecked {
                    from: context.accounts.trader_account_b.to_account_info(),
                    mint: context.accounts.mint_b.to_account_info(),
                    to: context.accounts.pool_account_b.to_account_info(),
                    authority: context.accounts.trader.to_account_info(),
                },
            ),
            output,
            context.accounts.mint_b.decimals,
        )?;
    }

    msg!(
        "Traded {} tokens ({} after fees) for {}",
        input,
        taxed_input,
        output
    );

    // Verify the invariant still holds
    // Reload accounts because of the CPIs
    // We tolerate if the new invariant is higher because it means a rounding error for LPs
    context.accounts.pool_account_a.reload()?;
    context.accounts.pool_account_b.reload()?;
    if invariant > context.accounts.pool_account_a.amount * context.accounts.pool_account_b.amount {
        return err!(TutorialError::InvariantViolated);
    }

    Ok(())
}

#[derive(Accounts)]
pub struct SwapExactTokensForTokens<'info> {
    #[account(
        seeds = [
            amm.id.as_ref()
        ],
        bump,
    )]
    pub amm: Account<'info, Amm>,

    #[account(
        seeds = [
            pool.amm.as_ref(),
            pool.mint_a.key().as_ref(),
            pool.mint_b.key().as_ref(),
        ],
        bump,
        has_one = amm,
        has_one = mint_a,
        has_one = mint_b,
    )]
    pub pool: Account<'info, Pool>,

    /// CHECK: Read only authority
    #[account(
        seeds = [
            pool.amm.as_ref(),
            mint_a.key().as_ref(),
            mint_b.key().as_ref(),
            AUTHORITY_SEED,
        ],
        bump,
    )]
    pub pool_authority: AccountInfo<'info>,

    /// The account doing the swap
    pub trader: Signer<'info>,

    pub mint_a: Box<Account<'info, Mint>>,

    pub mint_b: Box<Account<'info, Mint>>,

    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = pool_authority,
    )]
    pub pool_account_a: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = mint_b,
        associated_token::authority = pool_authority,
    )]
    pub pool_account_b: Box<Account<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint_a,
        associated_token::authority = trader,
    )]
    pub trader_account_a: Box<Account<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint_b,
        associated_token::authority = trader,
    )]
    pub trader_account_b: Box<Account<'info, TokenAccount>>,

    /// The account paying for all rents
    #[account(mut)]
    pub payer: Signer<'info>,

    /// Solana ecosystem accounts
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}
