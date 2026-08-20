use anchor_lang::prelude::*;

declare_id!("GUkjQmrLPFXXNK1bFLKt8XQi6g3TjxcHVspbjDoHvMG2");

/// The Pyth Receiver program that owns `PriceUpdateV2` accounts on devnet/mainnet.
pub const PYTH_RECEIVER_PROGRAM_ID: Address =
    anchor_lang::address!("rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ");

/// Maximum allowed age of a price update before it is rejected as stale.
/// Pyth's `publish_time` is a unix timestamp in seconds, so the age check
/// uses unix time rather than slots: seconds are the only freshness signal
/// the price message carries (this mirrors the official
/// `pyth-solana-receiver-sdk`'s `get_price_no_older_than`).
pub const MAXIMUM_PRICE_AGE_SECONDS: i64 = 60;

#[error_code]
pub enum PythExampleError {
    #[msg("The price update is older than the maximum allowed age")]
    PriceTooOld,
    #[msg("Computing the price update's age overflowed an i64")]
    MathOverflow,
}

#[program]
pub mod anchor_test {
    use super::*;

    pub fn read_price(context: &mut Context<ReadPriceAccountConstraints>) -> Result<()> {
        let price_update = &context.accounts.price_update;

        // Reject stale prices: a price that stopped updating is wrong.
        let price_age_seconds = Clock::get()?
            .unix_timestamp
            .checked_sub(price_update.price_message.publish_time)
            .ok_or(PythExampleError::MathOverflow)?;
        require!(
            price_age_seconds <= MAXIMUM_PRICE_AGE_SECONDS,
            PythExampleError::PriceTooOld
        );

        msg!("Price feed id: {:?}", price_update.price_message.feed_id);
        msg!("Price: {:?}", price_update.price_message.price);
        msg!("Confidence: {:?}", price_update.price_message.conf);
        msg!("Exponent: {:?}", price_update.price_message.exponent);
        msg!(
            "Publish Time: {:?}",
            price_update.price_message.publish_time
        );
        Ok(())
    }
}

#[derive(Accounts)]
pub struct ReadPriceAccountConstraints {
    pub price_update: BorshAccount<PriceUpdateV2>,
}

// ---------------------------------------------------------------------------
// Pyth `PriceUpdateV2` account, vendored from `pyth-solana-receiver-sdk`.
//
// The SDK's current release (2.0.0, checked August 2026) builds against
// `anchor-lang` 1.0.2, and this repository is on 2.0.0-rc.1, whose account
// wrappers are a different set of types. Importing the SDK's `PriceUpdateV2`
// would pull a second `anchor-lang` into the graph.
//
// The fields, order, and 8-byte discriminator below match the onchain account
// exactly, and it is owned by the Pyth Receiver program (see the `Owner` impl),
// so accounts written by Pyth deserialize unchanged. Import the SDK type once a
// release targeting `anchor-lang` 2.x ships.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, wincode::SchemaRead, wincode::SchemaWrite)]
pub enum VerificationLevel {
    /// Partially verified: only `num_signatures` of the Wormhole guardians
    /// were checked against the price update.
    Partial { num_signatures: u8 },
    /// Fully verified against the full guardian set.
    Full,
}

#[derive(Clone, Debug, PartialEq, Eq, wincode::SchemaRead, wincode::SchemaWrite)]
pub struct PriceFeedMessage {
    pub feed_id: [u8; 32],
    pub price: i64,
    pub conf: u64,
    pub exponent: i32,
    pub publish_time: i64,
    pub prev_publish_time: i64,
    pub ema_price: i64,
    pub ema_conf: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, wincode::SchemaRead, wincode::SchemaWrite)]
pub struct PriceUpdateV2 {
    pub write_authority: Address,
    pub verification_level: VerificationLevel,
    pub price_message: PriceFeedMessage,
    pub posted_slot: u64,
}

// Anchor's 8-byte discriminator: sha256("account:PriceUpdateV2")[..8].
impl anchor_lang::Discriminator for PriceUpdateV2 {
    const DISCRIMINATOR: &'static [u8] = &[34, 241, 35, 99, 157, 126, 244, 205];
}

// The account is created and owned by the Pyth Receiver program.
impl anchor_lang::Owner for PriceUpdateV2 {
    const OWNER: Address = PYTH_RECEIVER_PROGRAM_ID;
}
