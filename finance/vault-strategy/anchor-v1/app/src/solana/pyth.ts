// Raw account parsing that mirrors the program byte-for-byte (see programs/.../oracle.rs).
// The program reads Pyth PriceUpdateV2 and SPL token/mint accounts directly from bytes to
// dodge borsh/SDK version drift; the client must read them the same way to agree on NAV.

const PYTH_PRICE_OFFSET = 73; // i64 price
const PYTH_PUBLISH_TIME_OFFSET = 93; // i64 publish_time
const TOKEN_AMOUNT_OFFSET = 64; // u64 amount in an SPL token account
const MINT_DECIMALS_OFFSET = 44; // u8 decimals in an SPL mint account

export interface PythPrice {
  /** Price as an integer at exponent -8 (Pyth USD pairs). Divide by 10^8 for dollars. */
  price: bigint;
  /** Unix seconds of the last update. */
  publishTime: number;
}

function view(data: Uint8Array): DataView {
  return new DataView(data.buffer, data.byteOffset, data.byteLength);
}

export function parsePriceUpdateV2(data: Uint8Array): PythPrice {
  if (data.length < PYTH_PUBLISH_TIME_OFFSET + 8) {
    throw new Error("price feed account is too small to be a PriceUpdateV2");
  }
  const dv = view(data);
  return {
    price: dv.getBigInt64(PYTH_PRICE_OFFSET, true),
    publishTime: Number(dv.getBigInt64(PYTH_PUBLISH_TIME_OFFSET, true)),
  };
}

export function readTokenAmount(data: Uint8Array): bigint {
  if (data.length < TOKEN_AMOUNT_OFFSET + 8) throw new Error("not a token account");
  return view(data).getBigUint64(TOKEN_AMOUNT_OFFSET, true);
}

export function readMintDecimals(data: Uint8Array): number {
  if (data.length <= MINT_DECIMALS_OFFSET) throw new Error("not a mint account");
  return data[MINT_DECIMALS_OFFSET];
}

/** The program rejects prices older than 60s. `now` and `publishTime` are unix seconds. */
export function priceAgeSeconds(publishTime: number, now: number): number {
  return now - publishTime;
}
