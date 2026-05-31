use anchor_lang::prelude::*;

/// Lifecycle of a `Lease`. Transitions:
///   Listed      --take_lease-->     Active
///   Active      --return_lease-->   Closed
///   Active      --liquidate-->      Liquidated
///   Listed      --close_expired-->  Closed  (holder cancels unrented lease)
///   Active      --close_expired-->  Closed  (after end_timestamp, defaulted short_seller)
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug, InitSpace)]
pub enum LeaseStatus {
    Listed,
    Active,
    Liquidated,
    Closed,
}

#[account]
#[derive(InitSpace)]
pub struct Lease {
    /// Caller-supplied id so one holder can run many leases in parallel. The
    /// program-derived address is seeded by (LEASE_SEED, holder, lease_id).
    pub lease_id: u64,
    /// Account that listed the lease and receives the lease fee. Always set.
    pub holder: Pubkey,
    /// Account that took the lease. `Pubkey::default()` while `Listed`.
    pub short_seller: Pubkey,

    /// Mint of the tokens being leased out.
    pub leased_mint: Pubkey,
    /// Amount of leased tokens locked at creation. Used for repayment checks.
    pub leased_amount: u64,

    /// Mint of the collateral posted by the short_seller.
    pub collateral_mint: Pubkey,
    /// Collateral the short_seller posted (increases on top-up). Decreases as the lease fee
    /// is streamed out of the collateral vault.
    pub collateral_amount: u64,
    /// Collateral the short_seller must deposit up-front when taking the lease.
    pub required_collateral_amount: u64,

    /// Lease fee charged per second, denominated in collateral tokens and paid
    /// from the collateral vault to the holder on each `pay_lease_fee`.
    pub lease_fee_per_second: u64,
    /// Length of the lease, in seconds. Set at creation, used to compute
    /// `end_timestamp` when the lease activates.
    pub duration_seconds: i64,
    /// Unix timestamp when the lease becomes active (set on `take_lease`).
    pub start_timestamp: i64,
    /// Unix timestamp after which the lease expires. 0 while `Listed`.
    pub end_timestamp: i64,
    /// Last time the lease fee was settled. Lease fee accrues from here to `now.min(end_timestamp)`.
    pub last_paid_timestamp: i64,

    /// Required collateral value as a percentage of the leased value,
    /// expressed in basis points. 12_000 basis points = 120%.
    pub maintenance_margin_basis_points: u16,
    /// Share of the seized collateral paid to the keeper that liquidates the
    /// lease, expressed in basis points of `collateral_amount`.
    pub liquidation_bounty_basis_points: u16,

    /// Pyth `PriceUpdateV2.feed_id` that this lease is pinned to. The
    /// liquidation handler refuses price updates whose on-account `feed_id`
    /// does not match this value, so a keeper cannot swap in an unrelated
    /// feed (e.g. a cheaper or more volatile pair) to force a liquidation.
    /// Chosen by the holder at `create_lease`.
    pub feed_id: [u8; 32],

    /// Current lifecycle state.
    pub status: LeaseStatus,

    /// Bump seeds — stored so cross-program invocations can sign without re-deriving.
    pub bump: u8,
    pub leased_vault_bump: u8,
    pub collateral_vault_bump: u8,
}
