use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked},
};

use crate::{
    constants::{AUTHORITY_SEED, BASIS_POINTS_DIVISOR, CONFIG_SEED},
    errors::*,
    state::{Config, PoolConfig},
};

pub fn handle_swap_tokens(
    context: Context<SwapTokensAccounts>,
    input_is_token_a: bool,
    input_amount: u64,
    min_output_amount: u64,
) -> Result<()> {
    // Fail fast if the trader lacks the requested input balance. Previously this
    // silently clamped to the available balance, which broke slippage protection
    // for callers - their min_output_amount is computed against the requested
    // input, not the clamped one, so the trade could succeed with worse terms
    // than expected.
    if input_is_token_a && input_amount > context.accounts.token_a.amount {
        return err!(AmmError::InsufficientBalance);
    }
    if !input_is_token_a && input_amount > context.accounts.token_b.amount {
        return err!(AmmError::InsufficientBalance);
    }
    // Split the trading fee between LPs and the admin. The full fee is taken
    // off the input first (this is the standard Uniswap V2 mechanic). The
    // admin's slice is *not* transferred immediately - it accumulates as a
    // virtual claim on the input-side reserve, swept later by
    // `claim_admin_fees`. This saves a CPI per swap.
    //
    // u128 + checked arithmetic: `input * fee` can overflow u64 (both are
    // u64-sized in practice; fee is u16 but the multiplication grows fast).
    // Multiply before divide to preserve precision; floor on the divide is
    // protocol-favouring (the trader pays slightly more fee on rounding,
    // not less).
    let config = &context.accounts.config;
    let fee_amount = (input_amount as u128)
        .checked_mul(config.fee as u128)
        .ok_or(AmmError::MathOverflow)?
        .checked_div(BASIS_POINTS_DIVISOR as u128)
        .ok_or(AmmError::MathOverflow)?;
    let admin_portion = fee_amount
        .checked_mul(config.admin_share_bps as u128)
        .ok_or(AmmError::MathOverflow)?
        .checked_div(BASIS_POINTS_DIVISOR as u128)
        .ok_or(AmmError::MathOverflow)?;
    // Narrow back to u64 for storage / transfer. The fee can never exceed
    // `input` (`fee_amount <= input * 9999 / 10_000 < input`, and `input`
    // is u64), so the cast is safe — but use try_into anyway to make the
    // invariant explicit in the type system.
    let fee_amount: u64 = u64::try_from(fee_amount).map_err(|_| AmmError::MathOverflow)?;
    let admin_portion: u64 =
        u64::try_from(admin_portion).map_err(|_| AmmError::MathOverflow)?;
    // The LP portion stays in the pool reserves (as today - it's "less output
    // for the same input"), boosting the LP curve. The admin portion is
    // accounted for separately so it does *not* grow LP yield.
    let taxed_input = input_amount.checked_sub(fee_amount).ok_or(AmmError::MathOverflow)?;

    // Effective reserves = raw vault balance - admin's accumulated claim.
    // The constant-product curve runs on the LP-claimable portion only, so
    // the admin's outstanding fees do not contribute to LP yield and do not
    // distort the price.
    let pool_a = &context.accounts.pool_a;
    let pool_b = &context.accounts.pool_b;
    let pool_config = &context.accounts.pool_config;
    // checked_sub: admin_fees_owed is an invariant subset of the vault balance,
    // but a raw `-` would wrap silently on a BPF release build if that invariant
    // were ever violated, handing the curve a giant effective reserve.
    let effective_pool_a = pool_a
        .amount
        .checked_sub(pool_config.admin_fees_owed_a)
        .ok_or(AmmError::MathOverflow)?;
    let effective_pool_b = pool_b
        .amount
        .checked_sub(pool_config.admin_fees_owed_b)
        .ok_or(AmmError::MathOverflow)?;

    // Constant-product output formula:
    //   output = taxed_input * other_reserve / (this_reserve + taxed_input)
    // (where `this_reserve` is the input side, `other_reserve` the output
    // side, both using *effective* reserves).
    //
    // u128 + checked: the numerator `taxed_input * reserve` can fill the
    // full u128 (both factors are u64). Multiply before divide to keep
    // precision. Floor on the divide is protocol-favouring (the pool keeps
    // sub-base-unit rounding, the trader gets slightly less output) — same
    // direction as Uniswap V2.
    let (this_reserve, other_reserve) = if input_is_token_a {
        (effective_pool_a, effective_pool_b)
    } else {
        (effective_pool_b, effective_pool_a)
    };
    let numerator = (taxed_input as u128)
        .checked_mul(other_reserve as u128)
        .ok_or(AmmError::MathOverflow)?;
    let denominator = (this_reserve as u128)
        .checked_add(taxed_input as u128)
        .ok_or(AmmError::MathOverflow)?;
    let output_u128 = numerator
        .checked_div(denominator)
        .ok_or(AmmError::MathOverflow)?;
    let output: u64 = u64::try_from(output_u128).map_err(|_| AmmError::MathOverflow)?;

    // Trader's slippage protection: the caller passes the lowest output
    // they're willing to accept (computed offchain at quote time). If the
    // pool shifted between quoting and landing, we revert rather than fill
    // at the worse rate.
    require!(
        output >= min_output_amount,
        AmmError::SlippageExceeded
    );

    // Compute the invariant on the *effective* reserves before the trade.
    // Using raw balances here would let the admin's accumulated fees count
    // toward LP yield (wrong) and would cause the invariant check to pass
    // trivially even when the LP-claimable reserves shrunk.
    //
    // u128 + checked: each side is u64, so the product can fill the full
    // u128. Raw `*` on u64 would overflow at ~1.8e19 base units.
    let invariant = (effective_pool_a as u128)
        .checked_mul(effective_pool_b as u128)
        .ok_or(AmmError::MathOverflow)?;

    // Pre-copy seed bytes before the mutable borrow of pool_config below.
    // to_bytes() returns an owned [u8; 32] copy so there are no borrow conflicts.
    let authority_bump = context.bumps.pool_authority;
    let config_bytes = context.accounts.pool_config.config.to_bytes();
    let mint_a_bytes = context.accounts.mint_a.key().to_bytes();
    let mint_b_bytes = context.accounts.mint_b.key().to_bytes();

    // Effects: update admin_fees before CPIs (Checks-Effects-Interactions).
    // The fee always comes off the input side, so the admin's claim accumulates
    // in the same token.
    {
        let pool_config = &mut context.accounts.pool_config;
        if input_is_token_a {
            pool_config.admin_fees_owed_a = pool_config
                .admin_fees_owed_a
                .checked_add(admin_portion)
                .ok_or(AmmError::MathOverflow)?;
        } else {
            pool_config.admin_fees_owed_b = pool_config
                .admin_fees_owed_b
                .checked_add(admin_portion)
                .ok_or(AmmError::MathOverflow)?;
        }
    }

    // Interactions: CPIs after state has been updated.
    let authority_seeds = &[
        config_bytes.as_ref(),
        mint_a_bytes.as_ref(),
        mint_b_bytes.as_ref(),
        AUTHORITY_SEED,
        &[authority_bump],
    ];
    let signer_seeds = &[&authority_seeds[..]];
    if input_is_token_a {
        token_interface::transfer_checked(
            CpiContext::new(
                context.accounts.token_program.key(),
                TransferChecked {
                    from: context.accounts.token_a.to_account_info(),
                    mint: context.accounts.mint_a.to_account_info(),
                    to: context.accounts.pool_a.to_account_info(),
                    authority: context.accounts.trader.to_account_info(),
                },
            ),
            input_amount,
            context.accounts.mint_a.decimals,
        )?;
        token_interface::transfer_checked(
            CpiContext::new_with_signer(
                context.accounts.token_program.key(),
                TransferChecked {
                    from: context.accounts.pool_b.to_account_info(),
                    mint: context.accounts.mint_b.to_account_info(),
                    to: context.accounts.token_b.to_account_info(),
                    authority: context.accounts.pool_authority.to_account_info(),
                },
                signer_seeds,
            ),
            output,
            context.accounts.mint_b.decimals,
        )?;
    } else {
        token_interface::transfer_checked(
            CpiContext::new_with_signer(
                context.accounts.token_program.key(),
                TransferChecked {
                    from: context.accounts.pool_a.to_account_info(),
                    mint: context.accounts.mint_a.to_account_info(),
                    to: context.accounts.token_a.to_account_info(),
                    authority: context.accounts.pool_authority.to_account_info(),
                },
                signer_seeds,
            ),
            output,
            context.accounts.mint_a.decimals,
        )?;
        token_interface::transfer_checked(
            CpiContext::new(
                context.accounts.token_program.key(),
                TransferChecked {
                    from: context.accounts.token_b.to_account_info(),
                    mint: context.accounts.mint_b.to_account_info(),
                    to: context.accounts.pool_b.to_account_info(),
                    authority: context.accounts.trader.to_account_info(),
                },
            ),
            output,
            context.accounts.mint_b.decimals,
        )?;
    }

    msg!(
        "Traded {} tokens ({} after fees) for {} (admin slice {})",
        input_amount,
        taxed_input,
        output,
        admin_portion
    );

    // Verify the invariant still holds on the LP-claimable (effective)
    // reserves. This is THE most important defensive check: it catches
    // "I screwed up the swap math and accidentally gave the user too much"
    // bugs that no other test would catch. Defence in depth — runs *after*
    // the math (and after the transfers, once balances have been reloaded).
    //
    // We tolerate the new invariant being higher because it means a
    // rounding gain for LPs (and/or the LP portion of the fee enriching
    // the pool).
    //
    // u128 + checked: same overflow concern as the pre-trade invariant.
    context.accounts.pool_a.reload()?;
    context.accounts.pool_b.reload()?;
    let pool_config = &context.accounts.pool_config;
    let effective_pool_a_after = context
        .accounts
        .pool_a
        .amount
        .checked_sub(pool_config.admin_fees_owed_a)
        .ok_or(AmmError::MathOverflow)?;
    let effective_pool_b_after = context
        .accounts
        .pool_b
        .amount
        .checked_sub(pool_config.admin_fees_owed_b)
        .ok_or(AmmError::MathOverflow)?;
    let new_invariant = (effective_pool_a_after as u128)
        .checked_mul(effective_pool_b_after as u128)
        .ok_or(AmmError::MathOverflow)?;
    require!(new_invariant >= invariant, AmmError::InvariantViolated);

    Ok(())
}

#[derive(Accounts)]
pub struct SwapTokensAccounts<'info> {
    #[account(
        seeds = [CONFIG_SEED],
        bump,
    )]
    pub config: Account<'info, Config>,

    #[account(
        mut,
        seeds = [
            pool_config.config.as_ref(),
            pool_config.mint_a.key().as_ref(),
            pool_config.mint_b.key().as_ref(),
        ],
        bump,
        has_one = config,
        has_one = mint_a,
        has_one = mint_b,
    )]
    pub pool_config: Account<'info, PoolConfig>,

    /// CHECK: Read only authority
    #[account(
        seeds = [
            pool_config.config.as_ref(),
            mint_a.key().as_ref(),
            mint_b.key().as_ref(),
            AUTHORITY_SEED,
        ],
        bump,
    )]
    pub pool_authority: AccountInfo<'info>,

    /// The account doing the swap
    pub trader: Signer<'info>,

    pub mint_a: Box<InterfaceAccount<'info, Mint>>,

    pub mint_b: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = pool_authority,
        associated_token::token_program = token_program,
    )]
    pub pool_a: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = mint_b,
        associated_token::authority = pool_authority,
        associated_token::token_program = token_program,
    )]
    pub pool_b: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint_a,
        associated_token::authority = trader,
        associated_token::token_program = token_program,
    )]
    pub token_a: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint_b,
        associated_token::authority = trader,
        associated_token::token_program = token_program,
    )]
    pub token_b: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The account paying for all rents
    #[account(mut)]
    pub payer: Signer<'info>,

    /// Solana ecosystem accounts
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}
