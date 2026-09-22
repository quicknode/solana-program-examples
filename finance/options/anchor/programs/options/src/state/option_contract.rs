use anchor_lang::prelude::*;

/// Which right the holder buys.
#[derive(
    InitSpace, Clone, Copy, PartialEq, Eq, Debug, IdlType, wincode::SchemaRead, wincode::SchemaWrite,
)]
pub enum OptionKind {
    /// The right to buy the underlying at the strike. The writer's collateral
    /// is the underlying itself, so the call is covered.
    Call,
    /// The right to sell the underlying at the strike. The writer's collateral
    /// is the strike in the quote token, so the put is cash-secured.
    Put,
}

/// Where an option is in its life. Expiry is not a status: it is a comparison
/// of the clock against `expiry`, made by the handlers that care.
#[derive(
    InitSpace, Clone, Copy, PartialEq, Eq, Debug, IdlType, wincode::SchemaRead, wincode::SchemaWrite,
)]
pub enum OptionStatus {
    /// Written and collateralized, not yet sold. The writer may cancel.
    Listed,
    /// Sold. The holder may exercise before expiry; after expiry the writer
    /// reclaims the collateral.
    Held,
    /// The holder has paid the strike and taken the collateral. The writer
    /// collects the strike payment.
    Exercised,
}

/// One option: `contracts` identical contracts, written by one
/// writer, held by at most one holder. One PDA per (market, writer, id).
///
/// Every amount the option ever moves is a product of two of its integers,
/// so settlement never divides and never rounds:
///
/// - `contracts * underlying_per_contract` underlying minor units, which a
///   call writer posts and a call holder receives (or a put holder delivers).
/// - `contracts * strike_per_contract` quote minor units, which a put writer
///   posts and a put holder receives (or a call holder pays).
#[account(borsh)]
#[derive(InitSpace)]
pub struct OptionContract {
    /// Chosen by the writer, so one writer can have many options open.
    pub id: u64,

    pub market: Address,

    pub writer: Address,

    /// The buyer, once there is one. `Address::default()` while `Listed`.
    pub holder: Address,

    pub kind: OptionKind,

    pub status: OptionStatus,

    /// How many contracts the option holds. Bought and exercised as a whole.
    pub contracts: u64,

    /// Underlying minor units each contract is on (1 NVDAx = 1_000_000).
    pub underlying_per_contract: u64,

    /// Quote minor units each contract settles at: the strike, per contract,
    /// as an amount rather than a price, so exercise needs no decimals math.
    pub strike_per_contract: u64,

    /// Quote minor units the buyer pays the writer for the whole option.
    pub premium: u64,

    /// Unix timestamp after which the holder can no longer exercise and the
    /// writer may reclaim the collateral. Wall-clock time because an option's
    /// expiry is a calendar date the two parties agreed on, the same reason
    /// the fundraiser's deadline is a timestamp; the program reads no oracle,
    /// so slot-measured freshness never enters into it.
    pub expiry: i64,

    pub bump: u8,
}
