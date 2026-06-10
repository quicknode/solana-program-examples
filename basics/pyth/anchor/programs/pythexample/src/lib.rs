use anchor_lang::prelude::*;

declare_id!("GUkjQmrLPFXXNK1bFLKt8XQi6g3TjxcHVspbjDoHvMG2");

/// The Pyth Receiver program that owns `PriceUpdateV2` accounts on devnet/mainnet.
pub const PYTH_RECEIVER_PROGRAM_ID: Pubkey = pubkey!("rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ");

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

    pub fn read_price(context: Context<ReadPrice>) -> Result<()> {
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
pub struct ReadPrice<'info> {
    pub price_update: Account<'info, PriceUpdateV2>,
}

// ---------------------------------------------------------------------------
// Pyth `PriceUpdateV2` account, vendored from `pyth-solana-receiver-sdk`.
//
// The official `pyth-solana-receiver-sdk` is NOT Anchor 1.0 compatible (as of
// June 2026), so this example mirrors the `PriceUpdateV2` account type locally
// instead of importing it.
//
// Details: the latest `pyth-solana-receiver-sdk` (1.2.0) builds against
// `anchor-lang` 0.32 and pulls `pythnet-sdk` (2.3.1), which still derives
// borsh 0.10 on `PriceFeedMessage`. Anchor 0.32's `AnchorSerialize` /
// `AnchorDeserialize` derives require borsh 1.x, so the SDK's own
// `PriceUpdateV2` fails to compile:
//
//     error[E0277]: the trait bound
//     `pythnet_sdk::messages::PriceFeedMessage: BorshSerialize` is not satisfied
//
// No published `pyth-solana-receiver-sdk` targets `anchor-lang` 1.0 (which this
// repo standardizes on) and no `pythnet-sdk` release has migrated to borsh 1.x,
// so the dependency can't simply be upgraded. Tracked upstream at
// https://github.com/pyth-network/pyth-crosschain/issues/3756
//
// The fields, order, and 8-byte
// discriminator below match the onchain account exactly, and it is owned by
// the Pyth Receiver program (see the `Owner` impl), so accounts written by Pyth
// deserialize unchanged. Replace this with the SDK type once an Anchor 1.0 /
// borsh 1.x compatible `pyth-solana-receiver-sdk` release ships.
// ---------------------------------------------------------------------------

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum VerificationLevel {
    /// Partially verified: only `num_signatures` of the Wormhole guardians
    /// were checked against the price update.
    Partial { num_signatures: u8 },
    /// Fully verified against the full guardian set.
    Full,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
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

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct PriceUpdateV2 {
    pub write_authority: Pubkey,
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
    fn owner() -> Pubkey {
        PYTH_RECEIVER_PROGRAM_ID
    }
}

impl anchor_lang::AccountSerialize for PriceUpdateV2 {
    fn try_serialize<W: std::io::Write>(&self, writer: &mut W) -> Result<()> {
        writer
            .write_all(<Self as anchor_lang::Discriminator>::DISCRIMINATOR)
            .map_err(|_| anchor_lang::error::ErrorCode::AccountDidNotSerialize)?;
        AnchorSerialize::serialize(self, writer)
            .map_err(|_| anchor_lang::error::ErrorCode::AccountDidNotSerialize)?;
        Ok(())
    }
}

impl anchor_lang::AccountDeserialize for PriceUpdateV2 {
    fn try_deserialize(buf: &mut &[u8]) -> Result<Self> {
        let disc = <Self as anchor_lang::Discriminator>::DISCRIMINATOR;
        if buf.len() < disc.len() {
            return Err(anchor_lang::error::ErrorCode::AccountDiscriminatorNotFound.into());
        }
        if &buf[..disc.len()] != disc {
            return Err(anchor_lang::error::ErrorCode::AccountDiscriminatorMismatch.into());
        }
        Self::try_deserialize_unchecked(buf)
    }

    fn try_deserialize_unchecked(buf: &mut &[u8]) -> Result<Self> {
        let disc = <Self as anchor_lang::Discriminator>::DISCRIMINATOR;
        let mut data: &[u8] = &buf[disc.len()..];
        AnchorDeserialize::deserialize(&mut data)
            .map_err(|_| anchor_lang::error::ErrorCode::AccountDidNotDeserialize.into())
    }
}
