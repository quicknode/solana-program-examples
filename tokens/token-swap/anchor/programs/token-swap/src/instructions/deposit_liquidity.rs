use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, MintTo, Token, TokenAccount, TransferChecked},
};

use crate::{
    constants::{AUTHORITY_SEED, LIQUIDITY_SEED, MINIMUM_LIQUIDITY},
    errors::AmmError,
    state::PoolConfig,
};

/// Integer sqrt via Newton's method. Operates on `u128` so it can handle the
/// product `amount_a * amount_b` (each is a `u64`, product can fill the full
/// `u128`). Floors the result, which matches Uniswap V2's `Math.sqrt` and
/// keeps the initial-deposit LP-mint rounding in the pool's favour.
fn integer_sqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

pub fn handle_deposit_liquidity(
    context: Context<DepositLiquidityAccounts>,
    amount_a: u64,
    amount_b: u64,
    minimum_lp_tokens_out: u64,
) -> Result<()> {
    // Fail fast if the depositor lacks the requested balance. Previously this
    // silently clamped to the available balance, which broke slippage protection
    // for callers building on top - they expected their input amount to be the
    // amount actually deposited.
    if amount_a > context.accounts.token_a.amount
        || amount_b > context.accounts.token_b.amount
    {
        return err!(AmmError::InsufficientBalance);
    }
    let mut amount_a = amount_a;
    let mut amount_b = amount_b;

    // Clamp the caller's (amount_a, amount_b) to the current pool ratio.
    //
    // Callers pass `amount_a` / `amount_b` as *upper bounds* (their available
    // balance, or the most they want to commit). The pool is at a fixed
    // ratio, so at most one of the two amounts can be used in full; the other
    // is scaled down to match the current price. This mirrors Uniswap V2's
    // `mint()` pattern (UniswapV2Router._addLiquidity): try the first side at
    // its requested amount, compute what the other side needs at the current
    // ratio, and if it fits we're done — otherwise swap roles and try the
    // other side.
    //
    // We use the *effective* (LP-claimable) reserves, not the raw vault
    // balances, so the admin's accumulated fees don't drag the deposit ratio
    // off the LP-relevant price.
    //
    // All ratio math is in u128 with checked arithmetic — no floats for
    // money. The intermediate `amount_a * pool_b` can overflow u64 (both
    // factors are u64), but u128 absorbs that with room to spare.
    let pool_a = &context.accounts.pool_a;
    let pool_b = &context.accounts.pool_b;
    let pool_config = &context.accounts.pool_config;
    let effective_pool_a = pool_a.amount - pool_config.admin_fees_owed_a;
    let effective_pool_b = pool_b.amount - pool_config.admin_fees_owed_b;
    // Defining pool creation like this allows attackers to frontrun pool creation with bad ratios
    let pool_creation = effective_pool_a == 0 && effective_pool_b == 0;
    (amount_a, amount_b) = if pool_creation {
        // Add as is if there is no liquidity. Admin fees can't be owed yet
        // (no swap has happened), so the initial-deposit math is unchanged.
        (amount_a, amount_b)
    } else {
        // amount_b_required = amount_a * effective_pool_b / effective_pool_a.
        // Round down: this can only ever ask the depositor for *less* token B
        // than perfect-ratio, which favours the pool by a sub-base-unit and
        // matches Uniswap V2.
        let amount_b_required = (amount_a as u128)
            .checked_mul(effective_pool_b as u128)
            .unwrap()
            .checked_div(effective_pool_a as u128)
            .unwrap();
        if amount_b_required <= amount_b as u128 {
            // The depositor's `amount_b` is enough to cover the ratio; use
            // the full `amount_a` and clamp `amount_b` down.
            let amount_b_required = u64::try_from(amount_b_required).unwrap();
            (amount_a, amount_b_required)
        } else {
            // `amount_b` is the binding side; use the full `amount_b` and
            // clamp `amount_a` down to what the ratio needs.
            let amount_a_required = (amount_b as u128)
                .checked_mul(effective_pool_a as u128)
                .unwrap()
                .checked_div(effective_pool_b as u128)
                .unwrap();
            let amount_a_required = u64::try_from(amount_a_required).unwrap();
            (amount_a_required, amount_b)
        }
    };

    // After clamping, both sides must contribute something. If either side
    // rounds to zero the deposit is too small to register at the current
    // ratio (e.g. a depositor offering 1 base unit of A against a pool where
    // 1 A is worth less than 1 base unit of B). Fail rather than letting an
    // LP mint zero-priced shares.
    if !pool_creation && (amount_a == 0 || amount_b == 0) {
        return err!(AmmError::DepositAmountTooSmall);
    }

    // LP-mint math. Two branches:
    //   - Initial deposit (pool creation): `liquidity = sqrt(a * b) - MINIMUM_LIQUIDITY`.
    //     One-time bootstrap; the `MINIMUM_LIQUIDITY` floor is locked
    //     forever and prevents the first depositor from later draining the
    //     pool to a sub-base-unit ratio.
    //   - Subsequent deposit: `liquidity = min(a * supply / pool_a, b * supply / pool_b)`.
    //     This is the canonical Uniswap V2 formula: mint LP tokens in
    //     proportion to the depositor's share of each reserve, taking the
    //     smaller side as the binding constraint. After the ratio clamp
    //     above, both sides give the same result; `min` is kept as an
    //     invariant safety net and to match the published formula.
    //
    // All math is in `u128` with checked arithmetic. `amount * supply` can
    // overflow `u64` (both are `u64`), but `u128` absorbs it: max product
    // is `(2^64 - 1)^2 < 2^128`. We multiply before dividing to keep
    // precision, then round down (floor) so the pool keeps any sub-unit
    // rounding dust - protocol-favouring rounding, per the financial-math
    // rules.
    let liquidity: u64 = if pool_creation {
        let product = (amount_a as u128)
            .checked_mul(amount_b as u128)
            .ok_or(AmmError::MathOverflow)?;
        let sqrt_product = integer_sqrt(product);
        let sqrt_product_u64 = u64::try_from(sqrt_product)
            .map_err(|_| AmmError::MathOverflow)?;
        if sqrt_product_u64 < MINIMUM_LIQUIDITY {
            return err!(AmmError::DepositTooSmall);
        }
        sqrt_product_u64
            .checked_sub(MINIMUM_LIQUIDITY)
            .ok_or(AmmError::MathOverflow)?
    } else {
        let total_supply = context.accounts.liquidity_provider_mint.supply as u128;
        let liquidity_from_a = (amount_a as u128)
            .checked_mul(total_supply)
            .ok_or(AmmError::MathOverflow)?
            .checked_div(effective_pool_a as u128)
            .ok_or(AmmError::MathOverflow)?;
        let liquidity_from_b = (amount_b as u128)
            .checked_mul(total_supply)
            .ok_or(AmmError::MathOverflow)?
            .checked_div(effective_pool_b as u128)
            .ok_or(AmmError::MathOverflow)?;
        let liquidity = liquidity_from_a.min(liquidity_from_b);
        u64::try_from(liquidity).map_err(|_| AmmError::MathOverflow)?
    };

    if liquidity == 0 {
        // Subsequent deposit too small relative to existing LP supply.
        // (Initial deposits hit the `MINIMUM_LIQUIDITY` check above.)
        return err!(AmmError::DepositTooSmall);
    }

    // Depositor's slippage protection: the caller passes the lowest LP
    // amount they're willing to receive (computed off-chain at quote time).
    // If the pool ratio shifted between quoting and landing, the clamp will
    // have used a smaller pair of amounts and the LP-mint amount drops.
    // Revert rather than mint fewer LP tokens than the caller expects.
    //
    // This is the *lower-bound* slippage guard. The ratio clamp above is
    // the *upper-bound* guard (caps how much of each token can be spent).
    require!(
        liquidity >= minimum_lp_tokens_out,
        AmmError::DepositBelowMinimum
    );

    // Transfer tokens to the pool using transfer_checked. transfer_checked
    // includes the mint and decimals in the CPI, which guards callers against
    // decimal-mismatch bugs (and is the modern recommended path).
    token::transfer_checked(
        CpiContext::new(
            context.accounts.token_program.key(),
            TransferChecked {
                from: context.accounts.token_a.to_account_info(),
                mint: context.accounts.mint_a.to_account_info(),
                to: context.accounts.pool_a.to_account_info(),
                authority: context.accounts.depositor.to_account_info(),
            },
        ),
        amount_a,
        context.accounts.mint_a.decimals,
    )?;
    token::transfer_checked(
        CpiContext::new(
            context.accounts.token_program.key(),
            TransferChecked {
                from: context.accounts.token_b.to_account_info(),
                mint: context.accounts.mint_b.to_account_info(),
                to: context.accounts.pool_b.to_account_info(),
                authority: context.accounts.depositor.to_account_info(),
            },
        ),
        amount_b,
        context.accounts.mint_b.decimals,
    )?;

    // Mint the liquidity to user
    let authority_bump = context.bumps.pool_authority;
    let authority_seeds = &[
        &context.accounts.pool_config.config.to_bytes(),
        &context.accounts.mint_a.key().to_bytes(),
        &context.accounts.mint_b.key().to_bytes(),
        AUTHORITY_SEED,
        &[authority_bump],
    ];
    let signer_seeds = &[&authority_seeds[..]];
    token::mint_to(
        CpiContext::new_with_signer(
            context.accounts.token_program.key(),
            MintTo {
                mint: context.accounts.liquidity_provider_mint.to_account_info(),
                to: context.accounts.liquidity_provider_token.to_account_info(),
                authority: context.accounts.pool_authority.to_account_info(),
            },
            signer_seeds,
        ),
        liquidity,
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct DepositLiquidityAccounts<'info> {
    #[account(
        seeds = [
            pool_config.config.as_ref(),
            pool_config.mint_a.key().as_ref(),
            pool_config.mint_b.key().as_ref(),
        ],
        bump,
        has_one = mint_a,
        has_one = mint_b,
    )]
    pub pool_config: Box<Account<'info, PoolConfig>>,

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

    /// The account paying for all rents
    pub depositor: Signer<'info>,

    #[account(
        mut,
        seeds = [
            pool_config.config.as_ref(),
            mint_a.key().as_ref(),
            mint_b.key().as_ref(),
            LIQUIDITY_SEED,
        ],
        bump,
    )]
    pub liquidity_provider_mint: Box<Account<'info, Mint>>,

    pub mint_a: Box<Account<'info, Mint>>,

    pub mint_b: Box<Account<'info, Mint>>,

    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = pool_authority,
    )]
    pub pool_a: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = mint_b,
        associated_token::authority = pool_authority,
    )]
    pub pool_b: Box<Account<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = liquidity_provider_mint,
        associated_token::authority = depositor,
    )]
    pub liquidity_provider_token: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = depositor,
    )]
    pub token_a: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = mint_b,
        associated_token::authority = depositor,
    )]
    pub token_b: Box<Account<'info, TokenAccount>>,

    /// The account paying for all rents
    #[account(mut)]
    pub payer: Signer<'info>,

    /// Solana ecosystem accounts
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}
