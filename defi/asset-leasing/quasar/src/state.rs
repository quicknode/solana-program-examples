use quasar_lang::prelude::*;

/// Lifecycle of a lease. Stored as a single byte on `Lease` and driven by
/// the program — a user cannot write to it directly.
///
/// The final `Closed` / `Liquidated` states are set *before* the account is
/// closed by its handler, so the transaction log records the terminal state
/// even though the account itself disappears at the end of the transaction.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LeaseStatus {
    Listed = 0,
    Active = 1,
    Liquidated = 2,
    Closed = 3,
}

impl LeaseStatus {
    pub fn from_u8(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::Listed),
            1 => Some(Self::Active),
            2 => Some(Self::Liquidated),
            3 => Some(Self::Closed),
            _ => None,
        }
    }
}

/// Persistent per-lease state. Created on `create_lease`, closed on
/// `return_lease` / `liquidate` / `close_expired`.
///
/// Field order mirrors the Anchor version; integers are promoted to their
/// `PodXX` counterparts by the `#[account]` macro so the struct stays
/// alignment-1 and the onchain bytes match Anchor's little-endian layout
/// (after the one-byte Quasar discriminator replaces Anchor's 8-byte
/// sha256 prefix).
#[account(discriminator = 1)]
pub struct Lease {
    /// Caller-supplied id so one lessor can run many leases in parallel.
    pub lease_id: u64,

    /// Signer of `create_lease`; paid the lease fee and any final recovery.
    pub lessor: Address,

    /// Signer of `take_lease`. `Address::default()` while still `Listed`.
    pub lessee: Address,

    pub leased_mint: Address,
    /// Locked at creation, unchanging for the life of the lease.
    pub leased_amount: u64,

    pub collateral_mint: Address,
    /// Decreases as lease fee streams out; increases on `top_up_collateral`.
    pub collateral_amount: u64,
    /// What the lessee must post on `take_lease`.
    pub required_collateral_amount: u64,

    /// Denominated in collateral-mint base units per second.
    pub lease_fee_per_second: u64,
    pub duration_seconds: i64,
    /// `0` while `Listed`; `unix_timestamp` of `take_lease` while `Active`.
    pub start_timestamp: i64,
    /// `0` while `Listed`; `start_timestamp + duration_seconds` once `Active`.
    pub end_timestamp: i64,
    /// Lease fee accrues from here to `min(now, end_timestamp)`.
    pub last_paid_timestamp: i64,

    /// Collateral-over-debt ratio in basis points.
    /// `12_000` basis points = 120%. Capped at `MAX_MAINTENANCE_MARGIN_BASIS_POINTS`.
    pub maintenance_margin_basis_points: u16,
    /// Keeper's cut of the post-lease-fee collateral on liquidation, in basis
    /// points. Capped at `MAX_LIQUIDATION_BOUNTY_BASIS_POINTS` to stop a malicious
    /// lessor from draining the recovery pool via the bounty.
    pub liquidation_bounty_basis_points: u16,

    /// Pyth feed id this lease is pinned to at creation. Enforced on every
    /// `liquidate` so a keeper cannot swap in an unrelated feed to force an
    /// underwater verdict.
    pub feed_id: [u8; 32],

    /// Current lifecycle state. See [`LeaseStatus`].
    pub status: u8,

    pub bump: u8,
    pub leased_vault_bump: u8,
    pub collateral_vault_bump: u8,
}
