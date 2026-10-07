use anchor_lang::prelude::*;
use solana_sysvar::last_restart_slot::LastRestartSlot;

use crate::error::FundError;

/// Byte offset of the `verification_level` enum tag inside a Pyth
/// PriceUpdateV2 account: 8 discriminator + 32 write_authority = 40.
const PYTH_VERIFICATION_LEVEL_OFFSET: usize = 40;
/// Borsh tag of `VerificationLevel::Full`, a price verified against a quorum
/// of Pyth's guardian set. `Partial { num_signatures }` is tag 0 followed by a
/// one-byte signature count, so it encodes in two bytes rather than one and
/// moves every later field one byte along. The offsets below assume `Full`.
const PYTH_VERIFICATION_LEVEL_FULL: u8 = 1;
/// Byte offset of `price` (i64) inside a Pyth PriceUpdateV2 account:
///   8 discriminator + 32 write_authority + 1 verification_level + 32 feed_id = 73
const PYTH_PRICE_OFFSET: usize = 73;
/// Byte offset of `conf` (u64), the confidence interval: +8 bytes after price.
const PYTH_CONF_OFFSET: usize = PYTH_PRICE_OFFSET + 8; // 81
/// Byte offset of `exponent` (i32): price(8) + conf(8) = +16 bytes after price.
const PYTH_EXPONENT_OFFSET: usize = PYTH_PRICE_OFFSET + 8 + 8; // 89
/// Byte offset of `publish_time` (i64):
///   price(8) + conf(8) + exponent(4) = +20 bytes after price
const PYTH_PUBLISH_TIME_OFFSET: usize = PYTH_PRICE_OFFSET + 8 + 8 + 4; // 93
/// Byte offset of `posted_slot` (u64), the slot the update was posted in:
///   publish_time(8) + prev_publish_time(8) + ema_price(8) + ema_conf(8) = +32 bytes
const PYTH_POSTED_SLOT_OFFSET: usize = PYTH_PUBLISH_TIME_OFFSET + 8 + 8 + 8 + 8; // 125
/// Prices older than this (seconds) are rejected.
const MAX_PRICE_AGE_SECONDS: i64 = 60;
/// Widest confidence interval accepted, in basis points of the price (1%).
/// Deposits price shares and rebalance sets its swap floor from the Pyth
/// price, so a price that may be off by more than a typical 1% slippage
/// tolerance would quietly widen that tolerance. Major feeds usually quote
/// well under 0.1%; a band past 1% means the publishers disagree.
const MAX_CONFIDENCE_BPS: u128 = 100;

/// SPL token account layout: amount is a u64 at bytes 64..72. The base layout is
/// shared by the Classic Token Program and the Token Extensions Program, so this
/// reads either.
const TOKEN_AMOUNT_OFFSET: usize = 64;
/// `owner` Pubkey is at bytes 32..64.
const TOKEN_OWNER_OFFSET: usize = 32;
/// `mint` Pubkey is at bytes 0..32.
const TOKEN_MINT_OFFSET: usize = 0;

/// A positive Pyth price: `price * 10^exponent` dollars per whole token.
/// Crypto USD feeds use exponent -8 and US equity feeds -5, so the exponent is
/// read from the feed rather than assumed.
#[derive(Clone, Copy)]
pub struct OraclePrice {
    pub price: u128,
    pub exponent: i32,
}

/// Returns `(price, conf, exponent, publish_time, posted_slot)`.
fn read_pyth_raw(account_data: &[u8]) -> Result<(i64, u64, i32, i64, u64)> {
    if account_data.len() < PYTH_POSTED_SLOT_OFFSET + 8 {
        return err!(FundError::InvalidPriceFeed);
    }
    // Refuse anything but a fully verified update. A partially verified one
    // was signed by fewer than a quorum of the guardian set, and its longer
    // `verification_level` encoding would shift every offset below by a byte,
    // so its price would be read from the wrong bytes.
    require!(
        account_data[PYTH_VERIFICATION_LEVEL_OFFSET] == PYTH_VERIFICATION_LEVEL_FULL,
        FundError::PriceNotFullyVerified
    );
    let price = i64::from_le_bytes(
        account_data[PYTH_PRICE_OFFSET..PYTH_PRICE_OFFSET + 8]
            .try_into()
            .map_err(|_| FundError::InvalidPriceFeed)?,
    );
    let conf = u64::from_le_bytes(
        account_data[PYTH_CONF_OFFSET..PYTH_CONF_OFFSET + 8]
            .try_into()
            .map_err(|_| FundError::InvalidPriceFeed)?,
    );
    let exponent = i32::from_le_bytes(
        account_data[PYTH_EXPONENT_OFFSET..PYTH_EXPONENT_OFFSET + 4]
            .try_into()
            .map_err(|_| FundError::InvalidPriceFeed)?,
    );
    let publish_time = i64::from_le_bytes(
        account_data[PYTH_PUBLISH_TIME_OFFSET..PYTH_PUBLISH_TIME_OFFSET + 8]
            .try_into()
            .map_err(|_| FundError::InvalidPriceFeed)?,
    );
    let posted_slot = u64::from_le_bytes(
        account_data[PYTH_POSTED_SLOT_OFFSET..PYTH_POSTED_SLOT_OFFSET + 8]
            .try_into()
            .map_err(|_| FundError::InvalidPriceFeed)?,
    );
    Ok((price, conf, exponent, publish_time, posted_slot))
}

/// Validate a price feed account against the one the fund registered, then
/// return its positive, fresh price. `now` is the current unix timestamp.
/// A price whose confidence interval exceeds `MAX_CONFIDENCE_BPS` is rejected.
/// A price posted at or before the last cluster restart is rejected too, and
/// so is an update Pyth's guardian set did not fully verify.
pub fn load_price(
    price_feed: &AccountInfo,
    expected_key: &Pubkey,
    now: i64,
) -> Result<OraclePrice> {
    require_keys_eq!(price_feed.key(), *expected_key, FundError::InvalidPriceFeed);

    let data = price_feed.try_borrow_data()?;
    let (price, conf, exponent, publish_time, posted_slot) = read_pyth_raw(&data)?;

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
    let last_restart = LastRestartSlot::get()?.last_restart_slot;
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
pub fn read_token_amount(account: &AccountInfo) -> Result<u64> {
    let data = account.try_borrow_data()?;
    if data.len() < TOKEN_AMOUNT_OFFSET + 8 {
        return err!(FundError::InvalidVaultAccount);
    }
    Ok(u64::from_le_bytes(
        data[TOKEN_AMOUNT_OFFSET..TOKEN_AMOUNT_OFFSET + 8]
            .try_into()
            .map_err(|_| FundError::InvalidVaultAccount)?,
    ))
}

/// Read the `decimals` byte of a mint account. Offset 44 in the Mint layout
/// (mint_authority option 36 + supply 8), shared by both token programs.
pub fn read_mint_decimals(account: &AccountInfo) -> Result<u8> {
    let data = account.try_borrow_data()?;
    const MINT_DECIMALS_OFFSET: usize = 44;
    if data.len() <= MINT_DECIMALS_OFFSET {
        return err!(FundError::InvalidVaultAccount);
    }
    Ok(data[MINT_DECIMALS_OFFSET])
}

/// Read the `mint` and `owner` Pubkeys of a token account from its raw data.
pub fn read_token_mint_and_owner(account: &AccountInfo) -> Result<(Pubkey, Pubkey)> {
    let data = account.try_borrow_data()?;
    if data.len() < TOKEN_OWNER_OFFSET + 32 {
        return err!(FundError::InvalidVaultAccount);
    }
    let mint = Pubkey::try_from(&data[TOKEN_MINT_OFFSET..TOKEN_MINT_OFFSET + 32])
        .map_err(|_| FundError::InvalidVaultAccount)?;
    let owner = Pubkey::try_from(&data[TOKEN_OWNER_OFFSET..TOKEN_OWNER_OFFSET + 32])
        .map_err(|_| FundError::InvalidVaultAccount)?;
    Ok((mint, owner))
}

/// The fraction `numerator * 10^power / denominator` as a (numerator,
/// denominator) pair, for a power of either sign: a negative power multiplies
/// the denominator by `10^-power` instead. Multiplies before dividing.
fn mul_pow10_fraction(numerator: u128, power: i32, denominator: u128) -> Result<(u128, u128)> {
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
fn mul_pow10_div(numerator: u128, power: i32, denominator: u128) -> Result<u128> {
    let (numerator, denominator) = mul_pow10_fraction(numerator, power, denominator)?;
    numerator
        .checked_div(denominator)
        .ok_or(FundError::MathOverflow.into())
}

/// `numerator * 10^power / denominator`, rounded up.
fn mul_pow10_div_ceil(numerator: u128, power: i32, denominator: u128) -> Result<u128> {
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
) -> Result<u128> {
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
) -> Result<u128> {
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
) -> Result<u128> {
    let power = asset_decimals as i32 - price.exponent - usdc_decimals as i32;
    mul_pow10_div(usdc_amount, power, price.price)
}
