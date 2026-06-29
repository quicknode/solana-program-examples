use quasar_lang::{prelude::*, sysvars::Sysvar};

/// The Pyth Receiver program that owns `PriceUpdateV2` accounts on
/// devnet/mainnet (same constant as the Anchor twin).
pub const PYTH_RECEIVER_PROGRAM_ID: Address =
    address!("rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ");

/// Maximum allowed age of a price update before it is rejected as stale.
/// Pyth's `publish_time` is a unix timestamp in seconds, so the age check
/// uses unix time rather than slots: seconds are the only freshness signal
/// the price message carries (this mirrors the official
/// `pyth-solana-receiver-sdk`'s `get_price_no_older_than`).
pub const MAXIMUM_PRICE_AGE_SECONDS: i64 = 60;

/// Errors for reading Pyth price updates. Codes start at 6000, the same
/// offset Anchor uses for custom errors.
#[error_code]
pub enum PythExampleError {
    /// The price update account is not owned by the Pyth Receiver program,
    /// so its bytes cannot be trusted as a `PriceUpdateV2`.
    PriceUpdateNotOwnedByPythReceiver = 6000,
    /// The price update is older than `MAXIMUM_PRICE_AGE_SECONDS`.
    PriceTooOld,
    /// Computing the price update's age overflowed an i64.
    MathOverflow,
}

/// Byte layout offsets for a Pyth PriceUpdateV2 account:
///   [0..8]    Anchor discriminator
///   [8..40]   write_authority (Pubkey)
///   [40]      verification_level (u8)
///   [41..73]  feed_id ([u8; 32])
///   [73..81]  price (i64 LE)
///   [81..89]  conf (u64 LE)
///   [89..93]  exponent (i32 LE)
///   [93..101] publish_time (i64 LE)
const PRICE_OFFSET: usize = 73;
const CONF_OFFSET: usize = 81;
const EXPONENT_OFFSET: usize = 89;
const PUBLISH_TIME_OFFSET: usize = 93;
const MIN_DATA_LEN: usize = 101;

/// Accounts for reading a Pyth PriceUpdateV2 account.
/// Uses `UncheckedAccount` because Quasar does not have a built-in Pyth
/// account type; the `constraints(...)` check below enforces that the
/// account is owned by the Pyth Receiver program, so an attacker cannot
/// substitute an arbitrary account with plausible bytes.
#[derive(Accounts)]
pub struct ReadPriceAccountConstraints {
    /// The Pyth PriceUpdateV2 price update account.
    #[account(
        constraints(price_update.to_account_view().owner() == &PYTH_RECEIVER_PROGRAM_ID)
            @ PythExampleError::PriceUpdateNotOwnedByPythReceiver
    )]
    pub price_update: UncheckedAccount,
}

#[inline(always)]
pub fn handle_read_price(accounts: &mut ReadPriceAccountConstraints) -> Result<(), ProgramError> {
    let view = accounts.price_update.to_account_view();
    let data = unsafe { core::slice::from_raw_parts(view.data_ptr(), view.data_len()) };

    if data.len() < MIN_DATA_LEN {
        return Err(ProgramError::InvalidAccountData);
    }

    let _price = i64::from_le_bytes(
        data[PRICE_OFFSET..PRICE_OFFSET + 8]
            .try_into()
            .map_err(|_| ProgramError::InvalidAccountData)?,
    );
    let _conf = u64::from_le_bytes(
        data[CONF_OFFSET..CONF_OFFSET + 8]
            .try_into()
            .map_err(|_| ProgramError::InvalidAccountData)?,
    );
    let _exponent = i32::from_le_bytes(
        data[EXPONENT_OFFSET..EXPONENT_OFFSET + 4]
            .try_into()
            .map_err(|_| ProgramError::InvalidAccountData)?,
    );
    let publish_time = i64::from_le_bytes(
        data[PUBLISH_TIME_OFFSET..PUBLISH_TIME_OFFSET + 8]
            .try_into()
            .map_err(|_| ProgramError::InvalidAccountData)?,
    );

    // Reject stale prices: a price that stopped updating is wrong.
    let now: i64 = Clock::get()?.unix_timestamp.into();
    let price_age_seconds = now
        .checked_sub(publish_time)
        .ok_or(PythExampleError::MathOverflow)?;
    if price_age_seconds > MAXIMUM_PRICE_AGE_SECONDS {
        return Err(PythExampleError::PriceTooOld.into());
    }

    log("Pyth price feed data read successfully.");

    Ok(())
}
