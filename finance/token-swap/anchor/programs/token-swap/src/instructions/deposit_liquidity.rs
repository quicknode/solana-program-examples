use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{
    constants::{LIQUIDITY_SEED, MINIMUM_LIQUIDITY},
    errors::AmmError,
    liquidity::{deposit_and_mint_lp_tokens, LiquidityDepositAccounts},
    state::PoolConfig,
};

pub fn handle_deposit_liquidity(
    context: &mut Context<DepositLiquidityAccountConstraints>,
    amount_a: u64,
    amount_b: u64,
    minimum_lp_tokens_out: u64,
) -> Result<()> {
    // Fail fast if the depositor lacks the requested balance. Previously this
    // silently clamped to the available balance, which broke slippage protection
    // for callers building on top - they expected their input amount to be the
    // amount actually deposited.
    if amount_a > context.accounts.token_a.amount() || amount_b > context.accounts.token_b.amount()
    {
        return err!(AmmError::InsufficientBalance);
    }

    // Clamp the caller's (amount_a, amount_b) to the current pool ratio.
    //
    // Callers pass `amount_a` / `amount_b` as *upper bounds* (their available
    // balance, or the most they want to commit). The pool is at a fixed
    // ratio, so at most one of the two amounts can be used in full; the other
    // is scaled down to match the current price. This mirrors Uniswap V2's
    // `mint()` pattern (UniswapV2Router._addLiquidity): try the first side at
    // its requested amount, compute what the other side needs at the current
    // ratio, and if it fits we're done - otherwise swap roles and try the
    // other side.
    //
    // We use the *effective* (LP-claimable) reserves, not the raw vault
    // balances, so the admin's accumulated fees don't drag the deposit ratio
    // off the LP-relevant price.
    //
    // All ratio math is in u128 with checked arithmetic - no floats for
    // money. The intermediate `amount_a * pool_b` can overflow u64 (both
    // factors are u64), but u128 absorbs that with room to spare.
    let pool_a = &context.accounts.pool_a;
    let pool_b = &context.accounts.pool_b;
    let pool_config = &context.accounts.pool_config;
    // checked_sub: admin_fees_owed is an invariant subset of the vault balance;
    // a raw `-` would wrap silently on a BPF release build if that ever broke.
    let effective_pool_a = pool_a
        .amount()
        .checked_sub(pool_config.admin_fees_owed_a)
        .ok_or(AmmError::MathOverflow)?;
    let effective_pool_b = pool_b
        .amount()
        .checked_sub(pool_config.admin_fees_owed_b)
        .ok_or(AmmError::MathOverflow)?;

    // Every pool opens with its creator's deposit in `initialize_pool`, and
    // neither a withdrawal nor a swap can empty a reserve (the
    // `MINIMUM_LIQUIDITY` floor stays behind, and a swap's output is always
    // less than the reserve it comes from), so both effective reserves are
    // positive here. The check keeps the ratio math from dividing by zero if
    // that ever stopped being true, rather than letting a depositor set the
    // price of a pool that has none.
    require!(
        effective_pool_a > 0 && effective_pool_b > 0,
        AmmError::EmptyPoolReserve
    );

    // amount_b_required = amount_a * effective_pool_b / effective_pool_a.
    // Round down: this can only ever ask the depositor for *less* token B
    // than perfect-ratio, which favours the pool by a sub-base-unit and
    // matches Uniswap V2.
    let amount_b_required = (amount_a as u128)
        .checked_mul(effective_pool_b as u128)
        .ok_or(AmmError::MathOverflow)?
        .checked_div(effective_pool_a as u128)
        .ok_or(AmmError::MathOverflow)?;
    let (amount_a, amount_b) = if amount_b_required <= amount_b as u128 {
        // The depositor's `amount_b` is enough to cover the ratio; use
        // the full `amount_a` and clamp `amount_b` down.
        let amount_b_required =
            u64::try_from(amount_b_required).map_err(|_| AmmError::MathOverflow)?;
        (amount_a, amount_b_required)
    } else {
        // `amount_b` is the binding side; use the full `amount_b` and
        // clamp `amount_a` down to what the ratio needs.
        let amount_a_required = (amount_b as u128)
            .checked_mul(effective_pool_a as u128)
            .ok_or(AmmError::MathOverflow)?
            .checked_div(effective_pool_b as u128)
            .ok_or(AmmError::MathOverflow)?;
        let amount_a_required =
            u64::try_from(amount_a_required).map_err(|_| AmmError::MathOverflow)?;
        (amount_a_required, amount_b)
    };

    // After clamping, both sides must contribute something. If either side
    // rounds to zero the deposit is too small to register at the current
    // ratio (e.g. a depositor offering 1 base unit of A against a pool where
    // 1 A is worth less than 1 base unit of B). Fail rather than letting an
    // LP mint zero-priced shares.
    if amount_a == 0 || amount_b == 0 {
        return err!(AmmError::DepositAmountTooSmall);
    }

    // LP-mint math: `liquidity = min(a * total / pool_a, b * total / pool_b)`
    // with `total = lp_supply + MINIMUM_LIQUIDITY`. This is the canonical
    // Uniswap V2 formula: mint LP tokens in proportion to the depositor's
    // share of each reserve, taking the smaller side as the binding
    // constraint. After the ratio clamp above, both sides give the same
    // result; `min` is kept as an invariant safety net and to match the
    // published formula (Uniswap V2's `totalSupply` includes the floor it
    // minted to the zero address). `total` must be the same divisor
    // withdraw_liquidity uses: dividing by the bare mint supply here would
    // mint every depositor slightly less than they could redeem, and would
    // let a donation merely as large as a victim's deposit round that
    // deposit down to zero LP tokens, where counting the floor makes the
    // attacker donate at least `MINIMUM_LIQUIDITY + 1` times the deposit.
    // The floor itself is withheld from the creator's deposit in
    // `initialize_pool` (see `liquidity::initial_lp_amount`).
    //
    // All math is in `u128` with checked arithmetic. `amount * total` can
    // overflow `u64`, but `u128` absorbs it for any supply a real mint can
    // reach, and the checked multiply reports the rest. We multiply before
    // dividing to keep precision, then round down (floor) so the pool keeps
    // any sub-unit rounding dust - program-favouring rounding, per the
    // financial-math rules.
    let total_supply = (context.accounts.liquidity_provider_mint.supply() as u128)
        .checked_add(MINIMUM_LIQUIDITY as u128)
        .ok_or(AmmError::MathOverflow)?;
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
    let liquidity: u64 = u64::try_from(liquidity_from_a.min(liquidity_from_b))
        .map_err(|_| AmmError::MathOverflow)?;

    if liquidity == 0 {
        // Deposit too small relative to the existing LP supply.
        return err!(AmmError::DepositTooSmall);
    }

    // Depositor's slippage protection: the caller passes the lowest LP
    // amount they're willing to receive (computed offchain at quote time).
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

    // `pool_config` is loaded read-only here, so there is nothing to take
    // back after the mint releases its borrow.
    deposit_and_mint_lp_tokens(
        LiquidityDepositAccounts {
            token_program: &context.accounts.token_program,
            pool_config: &mut context.accounts.pool_config,
            mint_a: &context.accounts.mint_a,
            mint_b: &context.accounts.mint_b,
            pool_a: &mut context.accounts.pool_a,
            pool_b: &mut context.accounts.pool_b,
            depositor: &context.accounts.depositor,
            depositor_token_a: &mut context.accounts.token_a,
            depositor_token_b: &mut context.accounts.token_b,
            liquidity_provider_mint: &mut context.accounts.liquidity_provider_mint,
            liquidity_provider_token: &mut context.accounts.liquidity_provider_token,
        },
        amount_a,
        amount_b,
        liquidity,
    )
}

#[derive(Accounts)]
pub struct DepositLiquidityAccountConstraints {
    /// Owns both reserves and is the LP mint's authority; signs the mint_to.
    #[account(
        seeds = [
            pool_config.config.as_ref(),
            pool_config.mint_a.as_ref(),
            pool_config.mint_b.as_ref(),
        ],
        bump = pool_config.bump,
    )]
    pub pool_config: Box<BorshAccount<PoolConfig>>,

    /// Makes the deposit and receives the LP tokens.
    pub depositor: Signer,

    #[account(
        mut,
        seeds = [
            pool_config.config.as_ref(),
            mint_a.address().as_ref(),
            mint_b.address().as_ref(),
            LIQUIDITY_SEED,
        ],
        bump,
    )]
    pub liquidity_provider_mint: Box<InterfaceAccount<Mint>>,

    #[account(address = pool_config.mint_a)]
    pub mint_a: Box<InterfaceAccount<Mint>>,

    #[account(address = pool_config.mint_b)]
    pub mint_b: Box<InterfaceAccount<Mint>>,

    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = pool_config,
        associated_token::token_program = token_program,
    )]
    pub pool_a: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = mint_b,
        associated_token::authority = pool_config,
        associated_token::token_program = token_program,
    )]
    pub pool_b: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = liquidity_provider_mint,
        associated_token::authority = depositor,
        associated_token::token_program = token_program,
    )]
    pub liquidity_provider_token: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = depositor,
        associated_token::token_program = token_program,
    )]
    pub token_a: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = mint_b,
        associated_token::authority = depositor,
        associated_token::token_program = token_program,
    )]
    pub token_b: Box<InterfaceAccount<TokenAccount>>,

    /// The account paying for all rents
    #[account(mut)]
    pub payer: Signer,

    /// Solana ecosystem accounts
    pub token_program: Interface<'static, TokenInterface>,
    pub associated_token_program: Program<AssociatedToken>,
    pub system_program: Program<System>,
}
