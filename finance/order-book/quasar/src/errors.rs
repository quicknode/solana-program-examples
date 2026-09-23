use quasar_lang::prelude::*;

/// Program errors. `#[error_code]` assigns the numeric codes and generates the
/// `From<OrderBookError> for ProgramError` conversion that `?` and `require!`
/// rely on. Codes start at 6000, matching Anchor's custom-error base so the two
/// builds report the same numbers to clients.
#[error_code]
pub enum OrderBookError {
    InvalidPrice = 6000,
    OrderNotFound,
    MarketPaused,
    Unauthorized,
    OrderBookFull,
    TooManyOpenOrders,
    InvalidTickSize,
    InvalidBaseLotSize,
    InvalidQuoteLotSize,
    BelowMinOrderSize,
    OrderNotCancellable,
    NumericalOverflow,
    InvalidFeeBasisPoints,
    InvalidFeeVault,
    InvalidBaseVault,
    InvalidQuoteVault,
    InvalidBaseMint,
    InvalidQuoteMint,
    MakerAccountMismatch,
    MissingMakerAccounts,
    MakerOwnerMismatch,
    NotMarketAuthority,
    InvalidOrderBook,
    InvalidOrderBookOwner,
    OrderBookAlreadyInitialized,
    OrderIdMismatch,
    InvalidSide,
    /// A full side, and the worst resting order (with its owner's MarketUser)
    /// was not passed after the maker pairs.
    MissingEvictedAccounts,
    /// The order passed for eviction is not the side's worst, or the
    /// MarketUser passed is not its owner's.
    EvictedAccountMismatch,
}
