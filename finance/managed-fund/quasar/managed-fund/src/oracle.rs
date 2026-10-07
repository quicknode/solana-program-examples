use quasar_lang::{prelude::*, sysvars::Sysvar};

use crate::{errors::FundError, last_restart::LastRestartSlot};

/// Byte offset of the `verification_level` enum tag inside a Pyth
/// PriceUpdateV2 account: 8 discriminator + 32 write_authority = 40.
const PYTH_VERIFICATION_LEVEL_OFFSET: usize = 40;
/// Borsh tag of `VerificationLevel::Full`, a price verified against a quorum
/// of Pyth's guardian set. `Partial { num_signatures }` is tag 0 followed by a
/// one-byte signature count, so it encodes in two bytes rather than one and
/// moves every later field one byte along. The offsets below assume `Full`.
const PYTH_VERIFICATION_LEVEL_FULL: u8 = 1;
// Byte offset of `price` (i64) inside a Pyth PriceUpdateV2 account:
//   8 discriminator + 32 write_authority + 1 verification_level + 32 feed_id = 73
const PYTH_PRICE_OFFSET: usize = 73;
/// Byte offset of `conf` (u64), the confidence interval: +8 bytes after price.
const PYTH_CONF_OFFSET: usize = PYTH_PRICE_OFFSET + 8; // 81
/// Byte offset of `exponent` (i32): price(8) + conf(8) = +16 bytes after price.
const PYTH_EXPONENT_OFFSET: usize = PYTH_PRICE_OFFSET + 8 + 8; // 89
/// Byte offset of `publish_time` (i64): price(8) + conf(8) + exponent(4) after price.
const PYTH_PUBLISH_TIME_OFFSET: usize = PYTH_PRICE_OFFSET + 8 + 8 + 4; // 93
/// Byte offset of `posted_slot` (u64), the slot the update was posted in:
/// publish_time(8) + prev_publish_time(8) + ema_price(8) + ema_conf(8) after publish_time.
const PYTH_POSTED_SLOT_OFFSET: usize = PYTH_PUBLISH_TIME_OFFSET + 8 + 8 + 8 + 8; // 125
/// Prices older than this (seconds) are rejected.
const MAX_PRICE_AGE_SECONDS: i64 = 60;
/// Widest confidence interval accepted, in basis points of the price (1%).
/// Deposits price shares and rebalance sets its swap floor from the Pyth
/// price, so a price that may be off by more than a typical 1% slippage
/// tolerance would quietly widen that tolerance. Major feeds usually quote
/// well under 0.1%; a band past 1% means the publishers disagree.
const MAX_CONFIDENCE_BPS: u128 = 100;

// SPL token account layout, shared by the Classic and Extensions token programs.
const TOKEN_MINT_OFFSET: usize = 0; // mint: Pubkey [0..32]
const TOKEN_OWNER_OFFSET: usize = 32; // owner: Pubkey [32..64]
const TOKEN_AMOUNT_OFFSET: usize = 64; // amount: u64 [64..72]
                                       // Mint layout: mint_authority option(36) + supply(8) = 44.
const MINT_DECIMALS_OFFSET: usize = 44;

/// Borrow an account's raw data as a slice. Read-only; used for accounts that
/// are not deserialized into a Quasar wrapper (Pyth feeds, foreign token/mints).
fn account_data(view: &AccountView) -> &[u8] {
    // SAFETY: read-only view of the account's bytes, same pattern as the pyth
    // basics example. No mutable alias is taken.
    unsafe { core::slice::from_raw_parts(view.data_ptr(), view.data_len()) }
}

fn read_i64(data: &[u8], offset: usize) -> Result<i64, ProgramError> {
    let bytes: [u8; 8] = data
        .get(offset..offset + 8)
        .and_then(|slice| slice.try_into().ok())
        .ok_or(FundError::InvalidPriceFeed)?;
    Ok(i64::from_le_bytes(bytes))
}

fn read_i32(data: &[u8], offset: usize) -> Result<i32, ProgramError> {
    let bytes: [u8; 4] = data
        .get(offset..offset + 4)
        .and_then(|slice| slice.try_into().ok())
        .ok_or(FundError::InvalidPriceFeed)?;
    Ok(i32::from_le_bytes(bytes))
}

fn read_u64(data: &[u8], offset: usize) -> Result<u64, ProgramError> {
    let bytes: [u8; 8] = data
        .get(offset..offset + 8)
        .and_then(|slice| slice.try_into().ok())
        .ok_or(FundError::InvalidPriceFeed)?;
    Ok(u64::from_le_bytes(bytes))
}

/// A positive Pyth price: `price * 10^exponent` dollars per whole token.
/// Crypto USD feeds use exponent -8 and US equity feeds -5, so the exponent is
/// read from the feed rather than assumed.
#[derive(Clone, Copy)]
pub struct OraclePrice {
    pub price: u128,
    pub exponent: i32,
}

/// Validate a price feed account against the one the fund registered, then
/// return its positive, fresh price. `now` is the current unix timestamp.
/// A price whose confidence interval exceeds `MAX_CONFIDENCE_BPS` is rejected.
/// A price posted at or before the last cluster restart is rejected too, and
/// so is an update Pyth's guardian set did not fully verify.
pub fn load_price(
    price_feed: &AccountView,
    expected_key: &Address,
    now: i64,
) -> Result<OraclePrice, ProgramError> {
    if price_feed.address() != expected_key {
        return Err(FundError::InvalidPriceFeed.into());
    }

    let data = account_data(price_feed);
    if data.len() < PYTH_POSTED_SLOT_OFFSET + 8 {
        return Err(FundError::InvalidPriceFeed.into());
    }
    // Refuse anything but a fully verified update. A partially verified one
    // was signed by fewer than a quorum of the guardian set, and its longer
    // `verification_level` encoding would shift every offset below by a byte,
    // so its price would be read from the wrong bytes.
    require!(
        data[PYTH_VERIFICATION_LEVEL_OFFSET] == PYTH_VERIFICATION_LEVEL_FULL,
        FundError::PriceNotFullyVerified
    );
    let price = read_i64(data, PYTH_PRICE_OFFSET)?;
    let conf = read_u64(data, PYTH_CONF_OFFSET)?;
    let exponent = read_i32(data, PYTH_EXPONENT_OFFSET)?;
    let publish_time = read_i64(data, PYTH_PUBLISH_TIME_OFFSET)?;
    let posted_slot = read_u64(data, PYTH_POSTED_SLOT_OFFSET)?;

    require!(price > 0, FundError::NegativePrice);

    // Reject a price the oracle itself is unsure of: the confidence interval,
    // as a fraction of the price, must not exceed MAX_CONFIDENCE_BPS. `conf`
    // shares the price's exponent, so the ratio needs no scaling, and in u128
    // neither product can overflow.
    require!(
        (conf as u128) * 10_000 <= (price as u128) * MAX_CONFIDENCE_BPS,
        FundError::OracleConfidenceTooWide
    );

    require!(
        now.checked_sub(publish_time)
            .ok_or(FundError::MathOverflow)?
            <= MAX_PRICE_AGE_SECONDS,
        FundError::StalePriceFeed
    );

    // Restart handling. The staleness check above measures seconds against the
    // Clock's unix_timestamp, which under Alpenglow may advance by at most
    // twice the slot time elapsed since the parent block. A halt barely moves
    // the slot count, so after a restart the timestamp trails real time and a
    // price posted just before the halt can still pass the 60-second bound.
    // Reject any price posted at or before the restart slot; deposits and
    // rebalances then pause until Pyth posts again, rather than valuing the
    // vault at a pre-halt price. Zero means the cluster has never restarted.
    let last_restart = u64::from(LastRestartSlot::get()?.last_restart_slot);
    require!(
        last_restart == 0 || posted_slot > last_restart,
        FundError::PricePredatesRestart
    );

    Ok(OraclePrice {
        price: price as u128,
        exponent,
    })
}

/// Read the `amount` field of a token account from its raw data.
pub fn read_token_amount(account: &AccountView) -> Result<u64, ProgramError> {
    let data = account_data(account);
    let bytes: [u8; 8] = data
        .get(TOKEN_AMOUNT_OFFSET..TOKEN_AMOUNT_OFFSET + 8)
        .and_then(|slice| slice.try_into().ok())
        .ok_or(FundError::InvalidVaultAccount)?;
    Ok(u64::from_le_bytes(bytes))
}

/// Read the `decimals` byte of a mint account.
pub fn read_mint_decimals(account: &AccountView) -> Result<u8, ProgramError> {
    let data = account_data(account);
    data.get(MINT_DECIMALS_OFFSET)
        .copied()
        .ok_or_else(|| FundError::InvalidVaultAccount.into())
}

/// Read the `mint` and `owner` addresses of a token account from its raw data.
pub fn read_token_mint_and_owner(
    account: &AccountView,
) -> Result<(Address, Address), ProgramError> {
    let data = account_data(account);
    if data.len() < TOKEN_OWNER_OFFSET + 32 {
        return Err(FundError::InvalidVaultAccount.into());
    }
    let mut mint = [0u8; 32];
    mint.copy_from_slice(&data[TOKEN_MINT_OFFSET..TOKEN_MINT_OFFSET + 32]);
    let mut owner = [0u8; 32];
    owner.copy_from_slice(&data[TOKEN_OWNER_OFFSET..TOKEN_OWNER_OFFSET + 32]);
    Ok((Address::from(mint), Address::from(owner)))
}

/// The fraction `numerator * 10^power / denominator` as a (numerator,
/// denominator) pair, for a power of either sign: a negative power multiplies
/// the denominator by `10^-power` instead. Multiplies before dividing.
fn mul_pow10_fraction(
    numerator: u128,
    power: i32,
    denominator: u128,
) -> Result<(u128, u128), ProgramError> {
    let scale = 10u128
        .checked_pow(power.unsigned_abs())
        .ok_or(FundError::MathOverflow)?;
    if power >= 0 {
        Ok((
            numerator
                .checked_mul(scale)
                .ok_or(FundError::MathOverflow)?,
            denominator,
        ))
    } else {
        Ok((
            numerator,
            denominator
                .checked_mul(scale)
                .ok_or(FundError::MathOverflow)?,
        ))
    }
}

/// `numerator * 10^power / denominator`, floored.
fn mul_pow10_div(numerator: u128, power: i32, denominator: u128) -> Result<u128, ProgramError> {
    let (numerator, denominator) = mul_pow10_fraction(numerator, power, denominator)?;
    numerator
        .checked_div(denominator)
        .ok_or_else(|| FundError::MathOverflow.into())
}

/// `numerator * 10^power / denominator`, rounded up.
fn mul_pow10_div_ceil(
    numerator: u128,
    power: i32,
    denominator: u128,
) -> Result<u128, ProgramError> {
    let (numerator, denominator) = mul_pow10_fraction(numerator, power, denominator)?;
    if denominator == 0 {
        return Err(FundError::MathOverflow.into());
    }
    Ok(numerator.div_ceil(denominator))
}

/// Value of `amount` asset minor units in USDC minor units. The asset has
/// `asset_decimals`, USDC has `usdc_decimals`, and a whole asset is worth
/// `price * 10^exponent` dollars, so
/// value = amount * price * 10^(usdc_decimals + exponent - asset_decimals).
/// With six-decimal USDC, an eight-decimal asset and an exponent of -8, that is
/// amount * price / 10^10. Floored.
pub fn asset_value_in_usdc(
    amount: u128,
    price: OraclePrice,
    asset_decimals: u8,
    usdc_decimals: u8,
) -> Result<u128, ProgramError> {
    let power = usdc_decimals as i32 + price.exponent - asset_decimals as i32;
    mul_pow10_div(
        amount
            .checked_mul(price.price)
            .ok_or(FundError::MathOverflow)?,
        power,
        1,
    )
}

/// `asset_value_in_usdc` rounded up rather than down. Deposit prices new
/// shares against net asset value, and a NAV floored per asset is understated,
/// which would mint the depositor more shares than their USDC buys at the
/// expense of the holders already in the fund. Rounding each asset's value up
/// overstates NAV by under one minor unit per asset instead, so the share
/// count, floored again, rounds against the depositor.
pub fn asset_value_in_usdc_rounded_up(
    amount: u128,
    price: OraclePrice,
    asset_decimals: u8,
    usdc_decimals: u8,
) -> Result<u128, ProgramError> {
    let power = usdc_decimals as i32 + price.exponent - asset_decimals as i32;
    mul_pow10_div_ceil(
        amount
            .checked_mul(price.price)
            .ok_or(FundError::MathOverflow)?,
        power,
        1,
    )
}

/// The inverse of `asset_value_in_usdc`: how many asset minor units
/// `usdc_amount` USDC minor units buys at the oracle price,
/// usdc_amount * 10^(asset_decimals - exponent - usdc_decimals) / price. Floored.
pub fn usdc_to_asset_amount(
    usdc_amount: u128,
    price: OraclePrice,
    asset_decimals: u8,
    usdc_decimals: u8,
) -> Result<u128, ProgramError> {
    let power = asset_decimals as i32 - price.exponent - usdc_decimals as i32;
    mul_pow10_div(usdc_amount, power, price.price)
}
