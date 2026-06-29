use quasar_lang::prelude::*;

#[error_code]
pub enum AmmError {
    /// `create_config` was called with `fee >= 10_000` basis points (a fee of
    /// 100% or more would consume the whole input).
    // 6000 is the conventional Anchor-compatible starting offset for
    // program-specific error codes (Quasar's #[error_code] starts at 0
    // unless told otherwise; framework errors occupy 3000+).
    InvalidFee = 6000,
    /// `create_config` was called with `admin_share_bps >= 10_000`. The admin
    /// share is a basis-points fraction of the trading fee, so the admin
    /// cannot take more than the whole fee.
    AdminShareTooHigh,
    /// The initial deposit's geometric mean is below `MINIMUM_LIQUIDITY`, or a
    /// subsequent deposit is too small to mint any LP tokens.
    DepositTooSmall,
    /// Clamping the caller's amounts to the current pool ratio rounded one
    /// side down to zero; the pool cannot issue meaningful LP shares.
    DepositAmountTooSmall,
    /// The swap output is below the trader's `min_output_amount`. This is the
    /// trader's slippage guard against the pool shifting between quote and
    /// landing.
    SlippageExceeded,
    /// One side of the proportional withdrawal fell below the LP's specified
    /// minimum (`minimum_token_a_out` / `minimum_token_b_out`).
    WithdrawalBelowMinimum,
    /// The LP-token amount minted by a deposit fell below the depositor's
    /// `minimum_lp_tokens_out`. This is the lower-bound slippage guard; the
    /// ratio clamp is the upper-bound guard.
    DepositBelowMinimum,
    /// The constant-product invariant decreased across a swap.
    InvariantViolated,
    /// The caller asked to deposit or swap more tokens than they hold. The
    /// program fails fast instead of clamping to the balance, because
    /// clamping would invalidate the caller's slippage math.
    InsufficientBalance,
    /// `claim_admin_fees` was called while both fee accumulators are zero.
    NothingToClaim,
    /// A checked arithmetic operation overflowed or a u128 result did not fit
    /// back into u64.
    MathOverflow,
    /// The signer of `claim_admin_fees` does not match `Config.admin`.
    Unauthorized,
}
