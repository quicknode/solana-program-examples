mod create_collection;
mod mint_nft;
mod verify_collection;

pub use create_collection::*;
pub use mint_nft::*;
pub use verify_collection::*;

use quasar_lang::{cpi::CpiDynamic, prelude::*};

// Byte sizes of the Borsh encoding used by the Metaplex
// CreateMetadataAccountV3 instruction, used to size the CPI data buffer.
const BORSH_STRING_PREFIX: usize = core::mem::size_of::<u32>();
const BORSH_OPTION_TAG: usize = 1;
const BORSH_VEC_PREFIX: usize = core::mem::size_of::<u32>();
const BORSH_ENUM_TAG: usize = 1;
const BORSH_BOOL: usize = 1;
/// Creator = address (32) + verified (bool) + share (u8).
const CREATOR_SIZE: usize = core::mem::size_of::<Address>() + BORSH_BOOL + 1;
/// Collection = verified (bool) + key (32).
const COLLECTION_SIZE: usize = BORSH_BOOL + core::mem::size_of::<Address>();
/// CollectionDetails::V1 = enum tag + size (u64).
const COLLECTION_DETAILS_SIZE: usize = BORSH_ENUM_TAG + core::mem::size_of::<u64>();

/// Metaplex Token Metadata field limits, in bytes. These match the
/// `String<N>` capacities on the instruction arguments, so oversized
/// values are rejected at instruction decoding.
pub const MAX_NAME_LENGTH: usize = 32;
pub const MAX_SYMBOL_LENGTH: usize = 10;
pub const MAX_URI_LENGTH: usize = 200;

/// Instruction discriminator of CreateMetadataAccountV3 within the Metaplex
/// Token Metadata program.
const CREATE_METADATA_ACCOUNTS_V3_DISCRIMINATOR: u8 = 33;

/// Accounts taken by CreateMetadataAccountV3: metadata, mint, mint
/// authority, payer, update authority, system program, rent.
const CREATE_METADATA_ACCOUNT_COUNT: usize = 7;

/// Worst-case CreateMetadataAccountV3 instruction data length:
/// discriminator + DataV2 (name, symbol, uri, seller fee, one creator,
/// collection, uses) + is_mutable + collection_details.
const CREATE_METADATA_MAX_DATA: usize = 1
    + BORSH_STRING_PREFIX
    + MAX_NAME_LENGTH
    + BORSH_STRING_PREFIX
    + MAX_SYMBOL_LENGTH
    + BORSH_STRING_PREFIX
    + MAX_URI_LENGTH
    + core::mem::size_of::<u16>()
    + BORSH_OPTION_TAG
    + BORSH_VEC_PREFIX
    + CREATOR_SIZE
    + BORSH_OPTION_TAG
    + COLLECTION_SIZE
    + BORSH_OPTION_TAG
    + BORSH_BOOL
    + BORSH_OPTION_TAG
    + COLLECTION_DETAILS_SIZE;

const BORSH_OPTION_NONE: u8 = 0;
const BORSH_OPTION_SOME: u8 = 1;
/// CollectionDetails::V1 is the first enum variant.
const COLLECTION_DETAILS_V1_VARIANT: u8 = 0;
/// The PDA authority is the sole creator and receives the full royalty share.
const FULL_CREATOR_SHARE_PERCENT: u8 = 100;

/// Sequential writer over the fixed CPI data buffer. All writes are bounded
/// by `CREATE_METADATA_MAX_DATA` because the string arguments are capped by
/// their `String<N>` capacities and every other field is fixed size.
struct BorshWriter {
    buffer: [u8; CREATE_METADATA_MAX_DATA],
    offset: usize,
}

impl BorshWriter {
    fn new() -> Self {
        Self {
            buffer: [0; CREATE_METADATA_MAX_DATA],
            offset: 0,
        }
    }

    fn write_byte(&mut self, value: u8) {
        self.buffer[self.offset] = value;
        self.offset += 1;
    }

    fn write_slice(&mut self, bytes: &[u8]) {
        let end = self.offset + bytes.len();
        self.buffer[self.offset..end].copy_from_slice(bytes);
        self.offset = end;
    }

    fn write_string(&mut self, value: &str) {
        self.write_slice(&(value.len() as u32).to_le_bytes());
        self.write_slice(value.as_bytes());
    }

    fn data(&self) -> &[u8] {
        &self.buffer[..self.offset]
    }
}

/// Builds a Metaplex CreateMetadataAccountV3 CPI.
///
/// `quasar_metadata`'s `create_metadata_accounts_v3` helper always encodes
/// `creators`, `collection`, and `collection_details` as `None`. This program
/// needs all three (the PDA authority as verified creator, a collection
/// reference on minted NFTs, and sized collection details on the collection
/// NFT), so the instruction data is built here instead.
#[allow(clippy::too_many_arguments)]
#[inline(always)]
pub fn create_metadata_account_v3<'a>(
    token_metadata_program: &'a impl AsAccountView,
    metadata: &'a impl AsAccountView,
    mint: &'a impl AsAccountView,
    mint_authority: &'a impl AsAccountView,
    payer: &'a impl AsAccountView,
    update_authority: &'a impl AsAccountView,
    system_program: &'a impl AsAccountView,
    rent: &'a impl AsAccountView,
    name: &str,
    symbol: &str,
    uri: &str,
    creator: &Address,
    collection_mint: Option<&Address>,
    is_sized_collection: bool,
) -> Result<CpiDynamic<'a, CREATE_METADATA_ACCOUNT_COUNT, CREATE_METADATA_MAX_DATA>, ProgramError> {
    let mut cpi = CpiDynamic::<CREATE_METADATA_ACCOUNT_COUNT, CREATE_METADATA_MAX_DATA>::new(
        token_metadata_program.to_account_view().address(),
    );

    cpi.push_account(metadata.to_account_view(), false, true)?;
    cpi.push_account(mint.to_account_view(), false, false)?;
    cpi.push_account(mint_authority.to_account_view(), true, false)?;
    cpi.push_account(payer.to_account_view(), true, true)?;
    cpi.push_account(update_authority.to_account_view(), true, false)?;
    cpi.push_account(system_program.to_account_view(), false, false)?;
    cpi.push_account(rent.to_account_view(), false, false)?;

    let mut writer = BorshWriter::new();
    writer.write_byte(CREATE_METADATA_ACCOUNTS_V3_DISCRIMINATOR);

    // DataV2.name / symbol / uri
    writer.write_string(name);
    writer.write_string(symbol);
    writer.write_string(uri);

    // DataV2.seller_fee_basis_points
    writer.write_slice(&0u16.to_le_bytes());

    // DataV2.creators: Some([creator]) - verified, full share. Verified is
    // allowed because the creator (the PDA authority) signs the CPI.
    writer.write_byte(BORSH_OPTION_SOME);
    writer.write_slice(&1u32.to_le_bytes());
    writer.write_slice(creator.as_ref());
    writer.write_byte(true as u8);
    writer.write_byte(FULL_CREATOR_SHARE_PERCENT);

    // DataV2.collection: the (unverified) collection reference, if any.
    // Verification happens later via verify_collection.
    match collection_mint {
        Some(collection_key) => {
            writer.write_byte(BORSH_OPTION_SOME);
            writer.write_byte(false as u8);
            writer.write_slice(collection_key.as_ref());
        }
        None => writer.write_byte(BORSH_OPTION_NONE),
    }

    // DataV2.uses: None
    writer.write_byte(BORSH_OPTION_NONE);

    // is_mutable
    writer.write_byte(true as u8);

    // collection_details: Some(V1 { size: 0 }) marks a sized collection NFT.
    if is_sized_collection {
        writer.write_byte(BORSH_OPTION_SOME);
        writer.write_byte(COLLECTION_DETAILS_V1_VARIANT);
        writer.write_slice(&0u64.to_le_bytes());
    } else {
        writer.write_byte(BORSH_OPTION_NONE);
    }

    cpi.set_data(writer.data())?;
    Ok(cpi)
}
