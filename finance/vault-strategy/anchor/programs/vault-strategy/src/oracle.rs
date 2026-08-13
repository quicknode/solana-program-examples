use anchor_lang::prelude::*;

use crate::error::VaultError;

/// Byte offset of `price` (i64) inside a Pyth PriceUpdateV2 account:
///   8 discriminator + 32 write_authority + 1 verification_level + 32 feed_id = 73
const PYTH_PRICE_OFFSET: usize = 73;
/// Byte offset of `publish_time` (i64):
///   price(8) + conf(8) + exponent(4) = +20 bytes after price
const PYTH_PUBLISH_TIME_OFFSET: usize = PYTH_PRICE_OFFSET + 8 + 8 + 4; // 93
/// Pyth USD pairs use exponent -8 (price * 10^-8 = dollars per token).
pub const PYTH_PRICE_PRECISION: u128 = 100_000_000; // 10^8
/// Prices older than this (seconds) are rejected.
const MAX_PRICE_AGE_SECONDS: i64 = 60;

/// SPL token account layout: amount is a u64 at bytes 64..72. The base layout is
/// shared by the Classic Token Program and the Token Extensions Program, so this
/// reads either.
const TOKEN_AMOUNT_OFFSET: usize = 64;
/// `owner` Address is at bytes 32..64.
const TOKEN_OWNER_OFFSET: usize = 32;
/// `mint` Address is at bytes 0..32.
const TOKEN_MINT_OFFSET: usize = 0;

fn read_pyth_raw(account_data: &[u8]) -> Result<(i64, i64)> {
    if account_data.len() < PYTH_PUBLISH_TIME_OFFSET + 8 {
        return err!(VaultError::InvalidPriceFeed);
    }
    let price = i64::from_le_bytes(
        account_data[PYTH_PRICE_OFFSET..PYTH_PRICE_OFFSET + 8]
            .try_into()
            .map_err(|_| VaultError::InvalidPriceFeed)?,
    );
    let publish_time = i64::from_le_bytes(
        account_data[PYTH_PUBLISH_TIME_OFFSET..PYTH_PUBLISH_TIME_OFFSET + 8]
            .try_into()
            .map_err(|_| VaultError::InvalidPriceFeed)?,
    );
    Ok((price, publish_time))
}

/// Validate a price feed account against the one the strategy registered, then
/// return its positive, fresh price as u128. `now` is the current unix timestamp.
pub fn load_price(price_feed: &AccountView, expected_key: &Address, now: i64) -> Result<u128> {
    require_keys_eq!(
        price_feed.address(),
        *expected_key,
        VaultError::InvalidPriceFeed
    );

    let data = price_feed.try_borrow_data()?;
    let (price, publish_time) = read_pyth_raw(&data)?;

    require!(price > 0, VaultError::NegativePrice);
    require!(
        now.checked_sub(publish_time)
            .ok_or(VaultError::MathOverflow)?
            <= MAX_PRICE_AGE_SECONDS,
        VaultError::StalePriceFeed
    );

    Ok(price as u128)
}

/// Read the `amount` field of a token account from its raw data.
pub fn read_token_amount(account: &AccountView) -> Result<u64> {
    let data = account.try_borrow_data()?;
    if data.len() < TOKEN_AMOUNT_OFFSET + 8 {
        return err!(VaultError::InvalidVaultAccount);
    }
    Ok(u64::from_le_bytes(
        data[TOKEN_AMOUNT_OFFSET..TOKEN_AMOUNT_OFFSET + 8]
            .try_into()
            .map_err(|_| VaultError::InvalidVaultAccount)?,
    ))
}

/// Read the `decimals` byte of a mint account. Offset 44 in the Mint layout
/// (mint_authority option 36 + supply 8), shared by both token programs.
pub fn read_mint_decimals(account: &AccountView) -> Result<u8> {
    let data = account.try_borrow_data()?;
    const MINT_DECIMALS_OFFSET: usize = 44;
    if data.len() <= MINT_DECIMALS_OFFSET {
        return err!(VaultError::InvalidVaultAccount);
    }
    Ok(data[MINT_DECIMALS_OFFSET])
}

/// Read the `mint` and `owner` Pubkeys of a token account from its raw data.
pub fn read_token_mint_and_owner(account: &AccountView) -> Result<(Address, Address)> {
    let data = account.try_borrow_data()?;
    if data.len() < TOKEN_OWNER_OFFSET + 32 {
        return err!(VaultError::InvalidVaultAccount);
    }
    let mint = Address::try_from(&data[TOKEN_MINT_OFFSET..TOKEN_MINT_OFFSET + 32])
        .map_err(|_| VaultError::InvalidVaultAccount)?;
    let owner = Address::try_from(&data[TOKEN_OWNER_OFFSET..TOKEN_OWNER_OFFSET + 32])
        .map_err(|_| VaultError::InvalidVaultAccount)?;
    Ok((mint, owner))
}

/// Value of `amount` token minor units in USDC minor units, given a Pyth price.
/// Both USDC and the basket assets use 6 decimals, so the only scaling is the
/// Pyth exponent: value = amount * price / 10^8. Multiply before divide.
pub fn asset_value_in_usdc(amount: u64, price: u128) -> Result<u128> {
    (amount as u128)
        .checked_mul(price)
        .ok_or(VaultError::MathOverflow)?
        .checked_div(PYTH_PRICE_PRECISION)
        .ok_or(VaultError::MathOverflow)
        .map_err(Into::into)
}
