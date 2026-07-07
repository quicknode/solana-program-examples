use quasar_lang::prelude::*;

use crate::errors::VaultError;

// Byte offset of `price` (i64) inside a Pyth PriceUpdateV2 account:
//   8 discriminator + 32 write_authority + 1 verification_level + 32 feed_id = 73
const PYTH_PRICE_OFFSET: usize = 73;
// Byte offset of `publish_time` (i64): price(8) + conf(8) + exponent(4) after price.
const PYTH_PUBLISH_TIME_OFFSET: usize = PYTH_PRICE_OFFSET + 8 + 8 + 4; // 93
/// Pyth USD pairs use exponent -8 (price * 10^-8 = dollars per token).
pub const PYTH_PRICE_PRECISION: u128 = 100_000_000; // 10^8
/// Prices older than this (seconds) are rejected.
const MAX_PRICE_AGE_SECONDS: i64 = 60;

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
        .ok_or(VaultError::InvalidPriceFeed)?;
    Ok(i64::from_le_bytes(bytes))
}

/// Validate a price feed account against the one the strategy registered, then
/// return its positive, fresh price as u128. `now` is the current unix timestamp.
pub fn load_price(
    price_feed: &AccountView,
    expected_key: &Address,
    now: i64,
) -> Result<u128, ProgramError> {
    if price_feed.address() != expected_key {
        return Err(VaultError::InvalidPriceFeed.into());
    }

    let data = account_data(price_feed);
    if data.len() < PYTH_PUBLISH_TIME_OFFSET + 8 {
        return Err(VaultError::InvalidPriceFeed.into());
    }
    let price = read_i64(data, PYTH_PRICE_OFFSET)?;
    let publish_time = read_i64(data, PYTH_PUBLISH_TIME_OFFSET)?;

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
pub fn read_token_amount(account: &AccountView) -> Result<u64, ProgramError> {
    let data = account_data(account);
    let bytes: [u8; 8] = data
        .get(TOKEN_AMOUNT_OFFSET..TOKEN_AMOUNT_OFFSET + 8)
        .and_then(|slice| slice.try_into().ok())
        .ok_or(VaultError::InvalidVaultAccount)?;
    Ok(u64::from_le_bytes(bytes))
}

/// Read the `decimals` byte of a mint account.
pub fn read_mint_decimals(account: &AccountView) -> Result<u8, ProgramError> {
    let data = account_data(account);
    data.get(MINT_DECIMALS_OFFSET)
        .copied()
        .ok_or_else(|| VaultError::InvalidVaultAccount.into())
}

/// Read the `mint` and `owner` addresses of a token account from its raw data.
pub fn read_token_mint_and_owner(account: &AccountView) -> Result<(Address, Address), ProgramError> {
    let data = account_data(account);
    if data.len() < TOKEN_OWNER_OFFSET + 32 {
        return Err(VaultError::InvalidVaultAccount.into());
    }
    let mut mint = [0u8; 32];
    mint.copy_from_slice(&data[TOKEN_MINT_OFFSET..TOKEN_MINT_OFFSET + 32]);
    let mut owner = [0u8; 32];
    owner.copy_from_slice(&data[TOKEN_OWNER_OFFSET..TOKEN_OWNER_OFFSET + 32]);
    Ok((Address::from(mint), Address::from(owner)))
}

/// Value of `amount` token minor units in USDC minor units, given a Pyth price.
/// Both USDC and the basket assets use 6 decimals, so the only scaling is the
/// Pyth exponent: value = amount * price / 10^8. Multiply before divide.
pub fn asset_value_in_usdc(amount: u64, price: u128) -> Result<u128, ProgramError> {
    (amount as u128)
        .checked_mul(price)
        .ok_or(VaultError::MathOverflow)?
        .checked_div(PYTH_PRICE_PRECISION)
        .ok_or_else(|| VaultError::MathOverflow.into())
}
