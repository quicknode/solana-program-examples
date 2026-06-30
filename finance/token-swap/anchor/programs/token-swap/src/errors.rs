use anchor_lang::prelude::*;

#[error_code]
pub enum AmmError {
    #[msg("Invalid fee value")]
    InvalidFee,

    // Returned when `create_config` is called with `admin_share_bps >= 10_000`.
    // The admin share is a basis-points fraction of the trading fee, so values
    // at or above 10_000 are nonsensical (the admin can't take more than the
    // whole fee).
    #[msg("Admin share must be less than 10000 basis points")]
    AdminShareTooHigh,

    #[msg("Depositing too little liquidity")]
    DepositTooSmall,

    // Returned by `deposit_liquidity` when clamping the caller's amounts to the
    // current pool ratio rounds one side down to zero. That happens when the
    // deposit is so small (or so lopsided) that the pool can't issue meaningful
    // LP shares without rounding one of the contributions away. We fail rather
    // than mint zero-priced LP tokens.
    #[msg("Deposit amount too small for current pool ratio")]
    DepositAmountTooSmall,

    // Returned by `swap_tokens` when the computed `output_amount` is strictly
    // below the trader's `min_output_amount`. This is the trader's slippage
    // guard: between quoting and landing, the pool can shift (other traders,
    // sandwich attempts), so the trader passes the lowest output they're
    // willing to accept and the program reverts if reality is worse.
    #[msg("Swap output below minimum (slippage exceeded)")]
    SlippageExceeded,

    // Returned by `withdraw_liquidity` when either side of the proportional
    // withdrawal falls below the LP's specified minimum. This is the LP's
    // slippage guard: if a big swap drains one side of the pool between the
    // LP quoting their exit and the tx landing, the LP gets a different
    // mix than expected and can bail.
    #[msg("Withdrawal amount below minimum (slippage exceeded)")]
    WithdrawalBelowMinimum,

    // Returned by `deposit_liquidity` when the computed LP-token amount
    // falls below the depositor's specified minimum. This is the
    // *lower-bound* slippage guard on what the depositor receives. The
    // ratio clamp is the *upper-bound* guard (don't over-spend either
    // token); both are needed for full deposit slippage protection.
    #[msg("LP tokens minted below minimum (slippage exceeded)")]
    DepositBelowMinimum,

    #[msg("Invariant does not hold")]
    InvariantViolated,

    // Returned when a caller asks to deposit or swap more tokens than they hold.
    // Previously the program silently clamped to the available balance, which broke
    // slippage protection for callers (they expected their input to be the actual
    // amount used). We now fail fast so callers can react.
    #[msg("Requested amount exceeds available balance")]
    InsufficientBalance,

    // Returned by `claim_admin_fees` when both accumulators are zero. Reverting
    // (rather than silently no-op'ing) gives the admin a clear signal that the
    // call was wasted, and avoids the litesvm gotcha where two byte-identical
    // claim txs share a signature and the runtime rejects the second as
    // `AlreadyProcessed`. Callers should check the accumulators offchain
    // before submitting a claim.
    #[msg("No admin fees to claim")]
    NothingToClaim,

    // Returned by arithmetic helpers when a checked_* operation overflows or
    // underflows. We treat these as hard failures rather than masking them
    // with `.unwrap()` so the onchain logs name the failure mode.
    #[msg("Math overflow")]
    MathOverflow,

    // Returned by `create_pool` when `mint_a >= mint_b`. Requiring a strict
    // ascending order ensures each (mint_a, mint_b) pair has exactly one
    // canonical pool PDA - without it, a (X, Y) pool and a (Y, X) pool would
    // both be valid, fragmenting liquidity.
    #[msg("mint_a must be less than mint_b for canonical pool ordering")]
    InvalidMintOrder,

    // Returned by `swap_tokens` when either LP-claimable (effective) reserve is
    // zero. Swapping against an empty reserve would let the constant-product
    // curve drain the opposite side while the invariant check passes vacuously
    // (k = 0 >= 0), so the swap is rejected outright.
    #[msg("Pool reserves must both be positive to swap")]
    EmptyPoolReserve,
}
