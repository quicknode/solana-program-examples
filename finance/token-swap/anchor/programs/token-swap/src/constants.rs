use anchor_lang::prelude::*;

#[constant]
pub const MINIMUM_LIQUIDITY: u64 = 100;

/// Basis-points denominator. Fees and the admin's fee share are stored in
/// basis points (1 bp = 1/10_000), so dividing by this converts a bp value to
/// a fraction. Using the named constant keeps the 10_000 out of the math as a
/// bare literal.
#[constant]
pub const BASIS_POINTS_DIVISOR: u64 = 10_000;

#[constant]
pub const CONFIG_SEED: &[u8] = b"config";

#[constant]
pub const AUTHORITY_SEED: &[u8] = b"authority";

#[constant]
pub const LIQUIDITY_SEED: &[u8] = b"liquidity";
