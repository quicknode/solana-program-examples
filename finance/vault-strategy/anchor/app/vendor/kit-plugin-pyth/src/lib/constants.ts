import { type Address, address } from "@solana/kit";

export const HERMES_URL = "https://hermes.pyth.network";

export const PYTH_RECEIVER_PROGRAM_ID: Address = address("rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ");
export const WORMHOLE_PROGRAM_ID: Address = address("worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth");

// See https://docs.pyth.network/price-feeds/pythnet-price-feeds/on-chain
// for the Pyth price account binary format specification
export const PYTH_PRICE_ACCOUNT_MAGIC = 0xa1b2c3d4;
export const PYTH_ACCOUNT_TYPE_PRICE = 3;
// Minimum bytes needed to read through the aggregate pub_slot field
export const PYTH_PRICE_ACCOUNT_MIN_SIZE = 132;

// Byte offsets of the price account fields read by parsePythPriceAccountData,
// per the binary format specification linked above
export const PYTH_OFFSET_MAGIC = 0;
export const PYTH_OFFSET_ACCOUNT_TYPE = 8;
export const PYTH_OFFSET_EXPONENT = 20;
export const PYTH_OFFSET_EMA_PRICE = 48;
export const PYTH_OFFSET_EMA_CONFIDENCE = 56;
export const PYTH_OFFSET_TIMESTAMP = 64;
export const PYTH_OFFSET_AGGREGATE_PRICE = 100;
export const PYTH_OFFSET_AGGREGATE_CONFIDENCE = 108;
export const PYTH_OFFSET_STATUS = 116;
export const PYTH_OFFSET_PUBLISH_SLOT = 124;

// Merkle proof nodes in accumulator updates are 20-byte hashes
export const MERKLE_PROOF_NODE_SIZE = 20;

// Pyth aggregate price status codes
export const PYTH_STATUS_UNKNOWN = 0;
export const PYTH_STATUS_TRADING = 1;
export const PYTH_STATUS_HALTED = 2;
export const PYTH_STATUS_AUCTION = 3;

// Accumulator update binary format magic "PNAU" in big-endian
// See https://github.com/pyth-network/pyth-crosschain for the format spec
export const ACCUMULATOR_UPDATE_MAGIC = 0x504e4155;
export const ACCUMULATOR_UPDATE_TYPE_WORMHOLE_MERKLE = 0;

// Anchor discriminator for the postUpdateAtomic instruction:
// sha256("global:post_update_atomic")[0..8]
export const POST_UPDATE_ATOMIC_DISCRIMINATOR = new Uint8Array([49, 172, 84, 192, 175, 180, 52, 234]);

// Default treasury ID used in postUpdateAtomic instructions
export const DEFAULT_TREASURY_ID = 0;

// --- Pull oracle (PriceUpdateV2) ---------------------------------------------
// The accounts postPythPriceUpdate creates, and the ones onchain programs read via
// pyth-solana-receiver-sdk. Distinct from the legacy push-oracle price accounts
// above: these are Anchor accounts (8-byte discriminator, no magic field), so
// parsePythPriceAccountData cannot read them and parsePythPriceUpdateV2Data can.
//
// Layout (borsh), from pyth-solana-receiver-sdk's PriceUpdateV2:
//   discriminator      8
//   write_authority   32
//   verification_level 1 or 2   (see PYTH_VERIFICATION_LEVEL_* below)
//   price_message:
//     feed_id         32
//     price            8  i64
//     conf             8  u64
//     exponent         4  i32
//     publish_time     8  i64
//     prev_publish_time 8 i64
//     ema_price        8  i64
//     ema_conf         8  u64
//   posted_slot        8  u64
export const PRICE_UPDATE_V2_DISCRIMINATOR = new Uint8Array([34, 241, 35, 99, 157, 126, 244, 205]);

export const PYTH_OFFSET_V2_WRITE_AUTHORITY = 8;
export const PYTH_OFFSET_V2_VERIFICATION_LEVEL = 40;

// `verification_level` is a borsh enum, so it is NOT fixed width: the `Partial`
// variant carries a `num_signatures: u8` payload and `Full` carries none. Every
// field after it shifts by one byte between the two, which is why the parser
// reads this discriminant rather than assuming a constant offset for feed_id.
export const PYTH_VERIFICATION_LEVEL_PARTIAL = 0;
export const PYTH_VERIFICATION_LEVEL_FULL = 1;

// Offsets of the PriceFeedMessage fields, relative to the start of the message
// (i.e. add the byte offset where price_message begins).
export const PYTH_V2_MESSAGE_OFFSET_FEED_ID = 0;
export const PYTH_V2_MESSAGE_OFFSET_PRICE = 32;
export const PYTH_V2_MESSAGE_OFFSET_CONFIDENCE = 40;
export const PYTH_V2_MESSAGE_OFFSET_EXPONENT = 48;
export const PYTH_V2_MESSAGE_OFFSET_PUBLISH_TIME = 52;
export const PYTH_V2_MESSAGE_OFFSET_PREV_PUBLISH_TIME = 60;
export const PYTH_V2_MESSAGE_OFFSET_EMA_PRICE = 68;
export const PYTH_V2_MESSAGE_OFFSET_EMA_CONFIDENCE = 76;
export const PYTH_V2_MESSAGE_SIZE = 84;

// Price message type byte in accumulator update messages
export const ACCUMULATOR_PRICE_MESSAGE_TYPE = 0;

// Well-known Pyth feed IDs (hex format, without 0x prefix)
// Source: https://pyth.network/developers/price-feed-ids
export const PYTH_FEED_IDS = {
  SOL_USD: "ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d",
  BTC_USD: "e62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43",
  ETH_USD: "ff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace",
  USDC_USD: "eaa020c61cc479712813461ce153894a96a6c00b21ed0cfc2798d1f9a9e9c94a",
  USDT_USD: "2b89b9dc8fdf9f34709a5b106b472f0f39bb6ca9ce04b0fd7f2e971688e2e53b",
  BNB_USD: "2f95862b045670cd22bee3114c39763a4a08beeb663b145d283c31d7d1101c4f",
} as const;

// Addresses of legacy Pyth push-oracle price accounts. Pyth's live mainnet data comes from
// the pull oracle (Hermes + postPythPriceUpdate); these push-oracle accounts receive no
// updates and exist here only as decoding references for tests. Do not expect live data.
export const SOL_USD_PRICE_ACCOUNT: Address = address("H6ARHf6YXhGYeQfUzQNGFVe4yXUNsQhAAqaFxMbD1GRs");
export const BTC_USD_PRICE_ACCOUNT: Address = address("GVXRSBjFk6e931ingegDtaKZ2dYkK7sRoRZTYJTrEkKZ");
export const ETH_USD_PRICE_ACCOUNT: Address = address("JBu1AL4obBcCMqKBBxhpWCNUt136ijcuMZLFvTP7iWdB");
