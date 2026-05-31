/// program-derived address seed for the `Lease` account. Combined with the holder pubkey and a
/// u64 `lease_id` so one holder can run many leases in parallel.
pub const LEASE_SEED: &[u8] = b"lease";

/// program-derived address seed for the token vault that holds the leased tokens while the lease
/// is `Listed` and that accepts returned tokens on settlement.
pub const LEASED_VAULT_SEED: &[u8] = b"leased_vault";

/// program-derived address seed for the token vault that escrows the short_seller's collateral for the
/// life of the lease.
pub const COLLATERAL_VAULT_SEED: &[u8] = b"collateral_vault";

/// Denominator for basis-point (basis points) ratios used for the maintenance margin
/// and the liquidation bounty. 10_000 basis points = 100%.
pub const BASIS_POINTS_DENOMINATOR: u64 = 10_000;

/// Maximum allowed maintenance margin: 50_000 basis points = 500%. Prevents the holder
/// setting an impossible margin that would let them liquidate on day one.
pub const MAX_MAINTENANCE_MARGIN_BASIS_POINTS: u16 = 50_000;

/// Maximum liquidation bounty the keeper can claim: 2_000 basis points = 20%. Keeps
/// most of the collateral flowing to the holder on default.
pub const MAX_LIQUIDATION_BOUNTY_BASIS_POINTS: u16 = 2_000;

/// A Pyth price update is considered stale if its `publish_time` is older
/// than this many seconds versus the current onchain clock. 60 s matches the
/// default staleness window used in the Pyth SDK docs.
pub const PYTH_MAX_AGE_SECONDS: u64 = 60;
