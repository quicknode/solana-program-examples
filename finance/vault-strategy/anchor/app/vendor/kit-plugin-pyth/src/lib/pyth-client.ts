import type { Address, TransactionSendingSigner } from "@solana/kit";
import {
  AccountRole,
  addSignersToInstruction,
  generateKeyPairSigner,
  getAddressDecoder,
  getProgramDerivedAddress,
  SOLANA_ERROR__TRANSACTION__EXCEEDS_SIZE_LIMIT,
} from "@solana/kit";
import type { Connection } from "solana-kite";
import {
  ACCUMULATOR_PRICE_MESSAGE_TYPE,
  ACCUMULATOR_UPDATE_MAGIC,
  ACCUMULATOR_UPDATE_TYPE_WORMHOLE_MERKLE,
  DEFAULT_TREASURY_ID,
  HERMES_URL,
  MERKLE_PROOF_NODE_SIZE,
  POST_UPDATE_ATOMIC_DISCRIMINATOR,
  PRICE_UPDATE_V2_DISCRIMINATOR,
  PYTH_ACCOUNT_TYPE_PRICE,
  PYTH_OFFSET_ACCOUNT_TYPE,
  PYTH_OFFSET_AGGREGATE_CONFIDENCE,
  PYTH_OFFSET_AGGREGATE_PRICE,
  PYTH_OFFSET_EMA_CONFIDENCE,
  PYTH_OFFSET_EMA_PRICE,
  PYTH_OFFSET_EXPONENT,
  PYTH_OFFSET_MAGIC,
  PYTH_OFFSET_PUBLISH_SLOT,
  PYTH_OFFSET_STATUS,
  PYTH_OFFSET_TIMESTAMP,
  PYTH_OFFSET_V2_VERIFICATION_LEVEL,
  PYTH_OFFSET_V2_WRITE_AUTHORITY,
  PYTH_PRICE_ACCOUNT_MAGIC,
  PYTH_PRICE_ACCOUNT_MIN_SIZE,
  PYTH_RECEIVER_PROGRAM_ID,
  PYTH_STATUS_AUCTION,
  PYTH_STATUS_HALTED,
  PYTH_STATUS_TRADING,
  PYTH_STATUS_UNKNOWN,
  PYTH_V2_MESSAGE_OFFSET_CONFIDENCE,
  PYTH_V2_MESSAGE_OFFSET_EMA_CONFIDENCE,
  PYTH_V2_MESSAGE_OFFSET_EMA_PRICE,
  PYTH_V2_MESSAGE_OFFSET_EXPONENT,
  PYTH_V2_MESSAGE_OFFSET_FEED_ID,
  PYTH_V2_MESSAGE_OFFSET_PREV_PUBLISH_TIME,
  PYTH_V2_MESSAGE_OFFSET_PRICE,
  PYTH_V2_MESSAGE_OFFSET_PUBLISH_TIME,
  PYTH_V2_MESSAGE_SIZE,
  PYTH_VERIFICATION_LEVEL_FULL,
  PYTH_VERIFICATION_LEVEL_PARTIAL,
  WORMHOLE_PROGRAM_ID,
} from "./constants.js";
import type {
  PythConfig,
  PythFeedInfo,
  PythOnchainPriceData,
  PythPrice,
  PythPriceCallback,
  PythPriceFeed,
  PythPriceStatus,
  PythPriceUpdateV2,
  PythVerificationLevel,
} from "./types.js";

// Internal types for Hermes API responses
interface HermesPriceData {
  price: string;
  conf: string;
  expo: number;
  publish_time: number;
}

interface HermesParsedFeed {
  id: string;
  price: HermesPriceData;
  ema_price: HermesPriceData;
}

interface HermesResponse {
  binary: { data: Array<string> };
  parsed: Array<HermesParsedFeed>;
}

interface HermesFeedInfo {
  id: string;
  attributes: {
    asset_type: string;
    base: string;
    description: string;
    generic_symbol?: string;
    quote_currency: string;
    symbol: string;
    tenor?: string;
  };
}

interface AccumulatorUpdate {
  vaa: Uint8Array;
  // Map from feed ID (hex, no 0x) to its merkle price update data
  updatesByFeedId: Map<string, { message: Uint8Array; proof: Array<Uint8Array> }>;
}

// In JS it's possible to throw *anything*. A sensible programmer
// will only throw Errors but we must still check to satisfy
// TypeScript (and flag any craziness)
function ensureError(thrownObject: unknown): Error {
  if (thrownObject instanceof Error) {
    return thrownObject;
  }
  return new Error(`Non-Error thrown: ${String(thrownObject)}`);
}

function normalizeFeedId(feedId: string): string {
  return feedId.startsWith("0x") ? feedId.slice(2) : feedId;
}

function parseHermesPrice(data: HermesPriceData): PythPrice {
  const multiplier = 10 ** data.expo;
  return {
    price: Number(data.price) * multiplier,
    confidence: Number(data.conf) * multiplier,
    exponent: data.expo,
    publishTime: data.publish_time,
  };
}

export function parsePythPriceAccountData(data: Uint8Array): PythOnchainPriceData | null {
  if (data.length < PYTH_PRICE_ACCOUNT_MIN_SIZE) return null;

  const view = new DataView(data.buffer, data.byteOffset, data.byteLength);

  if (view.getUint32(PYTH_OFFSET_MAGIC, true) !== PYTH_PRICE_ACCOUNT_MAGIC) return null;
  if (view.getUint32(PYTH_OFFSET_ACCOUNT_TYPE, true) !== PYTH_ACCOUNT_TYPE_PRICE) return null;

  const exponent = view.getInt32(PYTH_OFFSET_EXPONENT, true);
  const emaPriceRaw = view.getBigInt64(PYTH_OFFSET_EMA_PRICE, true);
  const emaConfRaw = view.getBigUint64(PYTH_OFFSET_EMA_CONFIDENCE, true);
  const timestamp = view.getBigInt64(PYTH_OFFSET_TIMESTAMP, true);
  const priceRaw = view.getBigInt64(PYTH_OFFSET_AGGREGATE_PRICE, true);
  const confRaw = view.getBigUint64(PYTH_OFFSET_AGGREGATE_CONFIDENCE, true);
  const statusCode = view.getUint32(PYTH_OFFSET_STATUS, true);
  const slot = view.getBigUint64(PYTH_OFFSET_PUBLISH_SLOT, true);

  const multiplier = 10 ** exponent;

  const statusByCode: Record<number, PythPriceStatus> = {
    [PYTH_STATUS_UNKNOWN]: "unknown",
    [PYTH_STATUS_TRADING]: "trading",
    [PYTH_STATUS_HALTED]: "halted",
    [PYTH_STATUS_AUCTION]: "auction",
  };
  const status = statusByCode[statusCode] ?? "unknown";

  return {
    price: Number(priceRaw) * multiplier,
    confidence: Number(confRaw) * multiplier,
    exponent,
    emaPrice: Number(emaPriceRaw) * multiplier,
    emaConfidence: Number(emaConfRaw) * multiplier,
    status,
    publishTime: timestamp,
    slot,
  };
}

function toHex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

/**
 * Decode a pull-oracle PriceUpdateV2 account. Returns null when the data is too short
 * or does not start with the PriceUpdateV2 discriminator, so a caller can distinguish
 * "not this kind of account" from a decode that produced nonsense.
 */
export function parsePythPriceUpdateV2Data(data: Uint8Array): PythPriceUpdateV2 | null {
  if (data.length < PYTH_OFFSET_V2_VERIFICATION_LEVEL + 1) return null;
  if (!PRICE_UPDATE_V2_DISCRIMINATOR.every((byte, index) => data[index] === byte)) return null;

  const view = new DataView(data.buffer, data.byteOffset, data.byteLength);

  // `verification_level` is a borsh enum whose Partial variant carries a payload byte,
  // so the price message starts one byte later for Partial than for Full. Reading the
  // discriminant instead of assuming Full is what keeps a partially-verified update
  // from being decoded one byte out of alignment.
  const levelDiscriminant = view.getUint8(PYTH_OFFSET_V2_VERIFICATION_LEVEL);
  let verificationLevel: PythVerificationLevel;
  let messageStart: number;
  if (levelDiscriminant === PYTH_VERIFICATION_LEVEL_PARTIAL) {
    if (data.length < PYTH_OFFSET_V2_VERIFICATION_LEVEL + 2) return null;
    verificationLevel = {
      kind: "partial",
      numSignatures: view.getUint8(PYTH_OFFSET_V2_VERIFICATION_LEVEL + 1),
    };
    messageStart = PYTH_OFFSET_V2_VERIFICATION_LEVEL + 2;
  } else if (levelDiscriminant === PYTH_VERIFICATION_LEVEL_FULL) {
    verificationLevel = { kind: "full" };
    messageStart = PYTH_OFFSET_V2_VERIFICATION_LEVEL + 1;
  } else {
    return null;
  }

  // posted_slot (u64) follows the price message.
  if (data.length < messageStart + PYTH_V2_MESSAGE_SIZE + 8) return null;

  const feedIdStart = messageStart + PYTH_V2_MESSAGE_OFFSET_FEED_ID;
  const addressDecoder = getAddressDecoder();

  return {
    feedId: toHex(data.subarray(feedIdStart, feedIdStart + 32)),
    writeAuthority: addressDecoder.decode(
      data.subarray(PYTH_OFFSET_V2_WRITE_AUTHORITY, PYTH_OFFSET_V2_WRITE_AUTHORITY + 32),
    ),
    verificationLevel,
    price: view.getBigInt64(messageStart + PYTH_V2_MESSAGE_OFFSET_PRICE, true),
    confidence: view.getBigUint64(messageStart + PYTH_V2_MESSAGE_OFFSET_CONFIDENCE, true),
    exponent: view.getInt32(messageStart + PYTH_V2_MESSAGE_OFFSET_EXPONENT, true),
    publishTime: view.getBigInt64(messageStart + PYTH_V2_MESSAGE_OFFSET_PUBLISH_TIME, true),
    previousPublishTime: view.getBigInt64(messageStart + PYTH_V2_MESSAGE_OFFSET_PREV_PUBLISH_TIME, true),
    emaPrice: view.getBigInt64(messageStart + PYTH_V2_MESSAGE_OFFSET_EMA_PRICE, true),
    emaConfidence: view.getBigUint64(messageStart + PYTH_V2_MESSAGE_OFFSET_EMA_CONFIDENCE, true),
    postedSlot: view.getBigUint64(messageStart + PYTH_V2_MESSAGE_SIZE, true),
  };
}

// Parses the binary accumulator update from Hermes.
// Format: https://github.com/pyth-network/pyth-crosschain (accumulator message spec)
function parseAccumulatorUpdate(data: Uint8Array): AccumulatorUpdate {
  const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
  let offset = 0;

  const magic = view.getUint32(offset, false);
  offset += 4;
  if (magic !== ACCUMULATOR_UPDATE_MAGIC) {
    throw new Error(`Invalid accumulator magic: 0x${magic.toString(16)}`);
  }

  const majorVersion = view.getUint8(offset++);
  if (majorVersion !== 1) {
    throw new Error(`Unsupported accumulator version: ${majorVersion}`);
  }
  offset++; // minor version
  const trailingHeaderSize = view.getUint8(offset++);
  offset += trailingHeaderSize;

  const updateType = view.getUint8(offset++);
  if (updateType !== ACCUMULATOR_UPDATE_TYPE_WORMHOLE_MERKLE) {
    throw new Error(`Unsupported accumulator update type: ${updateType}`);
  }

  const vaaLength = view.getUint16(offset, false);
  offset += 2;
  const vaa = data.slice(offset, offset + vaaLength);
  offset += vaaLength;

  const numUpdates = view.getUint8(offset++);
  const updatesByFeedId = new Map<string, { message: Uint8Array; proof: Array<Uint8Array> }>();

  for (let updateIndex = 0; updateIndex < numUpdates; updateIndex++) {
    const messageLength = view.getUint16(offset, false);
    offset += 2;
    const message = data.slice(offset, offset + messageLength);
    offset += messageLength;

    const proofCount = view.getUint8(offset++);
    const proof: Array<Uint8Array> = [];
    for (let proofIndex = 0; proofIndex < proofCount; proofIndex++) {
      proof.push(data.slice(offset, offset + MERKLE_PROOF_NODE_SIZE));
      offset += MERKLE_PROOF_NODE_SIZE;
    }

    // Extract feed ID from the price message (type byte + 32-byte feed ID)
    if (message.length >= 33 && message[0] === ACCUMULATOR_PRICE_MESSAGE_TYPE) {
      const feedId = Buffer.from(message.slice(1, 33)).toString("hex");
      updatesByFeedId.set(feedId, { message, proof });
    }
  }

  return { vaa, updatesByFeedId };
}

// Borsh-encodes a byte vector: 4-byte LE length prefix + bytes
function borshEncodeBytes(bytes: Uint8Array): Uint8Array {
  const result = new Uint8Array(4 + bytes.length);
  new DataView(result.buffer).setUint32(0, bytes.length, true);
  result.set(bytes, 4);
  return result;
}

// Borsh-encodes a Vec<[u8; 20]>: 4-byte LE count + (count * 20) bytes
function borshEncodeProofVec(proofs: Array<Uint8Array>): Uint8Array {
  const result = new Uint8Array(4 + proofs.length * MERKLE_PROOF_NODE_SIZE);
  new DataView(result.buffer).setUint32(0, proofs.length, true);
  let offset = 4;
  for (const proof of proofs) {
    result.set(proof.slice(0, MERKLE_PROOF_NODE_SIZE), offset);
    offset += MERKLE_PROOF_NODE_SIZE;
  }
  return result;
}

function buildPostUpdateAtomicData(
  vaa: Uint8Array,
  message: Uint8Array,
  proof: Array<Uint8Array>,
  treasuryId: number,
): Uint8Array {
  const parts = [
    POST_UPDATE_ATOMIC_DISCRIMINATOR,
    borshEncodeBytes(vaa),
    borshEncodeBytes(message),
    borshEncodeProofVec(proof),
    new Uint8Array([treasuryId]),
  ];
  const totalLength = parts.reduce((sum, part) => sum + part.length, 0);
  const result = new Uint8Array(totalLength);
  let offset = 0;
  for (const part of parts) {
    result.set(part, offset);
    offset += part.length;
  }
  return result;
}

// Anchor discriminator for reclaimRent: sha256("global:reclaim_rent")[0..8]
const RECLAIM_RENT_DISCRIMINATOR = new Uint8Array([218, 200, 19, 197, 227, 89, 192, 22]);

const SYSTEM_PROGRAM_ADDRESS = "11111111111111111111111111111111" as Address;

export class PythClient {
  private hermesUrl: string;
  private connection: Connection;

  constructor(connection: Connection, config: PythConfig = {}) {
    this.connection = connection;
    this.hermesUrl = config.hermesUrl ?? HERMES_URL;
  }

  async getPythPriceFeed(feedId: string): Promise<PythPriceFeed | null> {
    const normalizedId = normalizeFeedId(feedId);
    const { feeds } = await this.fetchHermesLatest([normalizedId]);
    return feeds.get(normalizedId) ?? null;
  }

  async getPythPriceFeeds(feedIds: Array<string>): Promise<Map<string, PythPriceFeed>> {
    const normalizedIds = feedIds.map(normalizeFeedId);
    const { feeds } = await this.fetchHermesLatest(normalizedIds);
    return feeds;
  }

  private async fetchHermesLatest(
    feedIds: Array<string>,
  ): Promise<{ feeds: Map<string, PythPriceFeed>; binaryData: Uint8Array | null }> {
    const params = new URLSearchParams();
    for (const feedId of feedIds) {
      params.append("ids[]", `0x${feedId}`);
    }
    params.set("parsed", "true");
    params.set("encoding", "base64");

    const response = await fetch(`${this.hermesUrl}/v2/updates/price/latest?${params}`);
    if (!response.ok) {
      throw new Error(`Hermes API error: ${response.status} ${response.statusText}`);
    }

    const data = (await response.json()) as HermesResponse;

    const feeds = new Map<string, PythPriceFeed>();
    for (const feed of data.parsed) {
      const normalizedId = normalizeFeedId(feed.id);
      feeds.set(normalizedId, {
        id: normalizedId,
        price: parseHermesPrice(feed.price),
        emaPrice: parseHermesPrice(feed.ema_price),
      });
    }

    const binaryData =
      data.binary?.data?.[0] != null ? new Uint8Array(Buffer.from(data.binary.data[0], "base64")) : null;

    return { feeds, binaryData };
  }

  async getPythOnchainPrice(priceAccountAddress: Address): Promise<PythOnchainPriceData | null> {
    // Returns null when the account does not exist. RPC and decode errors are allowed to
    // propagate rather than being silently swallowed into an indistinguishable null.
    const accountInfo = await this.connection.rpc.getAccountInfo(priceAccountAddress, { encoding: "base64" }).send();
    if (!accountInfo.value) return null;

    const [encodedData] = accountInfo.value.data as readonly [string, string];
    const rawBytes = new Uint8Array(Buffer.from(encodedData, "base64"));
    return parsePythPriceAccountData(rawBytes);
  }

  async getPythPriceUpdateV2(priceUpdateAddress: Address): Promise<PythPriceUpdateV2 | null> {
    // Mirrors getPythOnchainPrice: null for a missing account, errors propagate.
    const accountInfo = await this.connection.rpc.getAccountInfo(priceUpdateAddress, { encoding: "base64" }).send();
    if (!accountInfo.value) return null;

    const [encodedData] = accountInfo.value.data as readonly [string, string];
    const rawBytes = new Uint8Array(Buffer.from(encodedData, "base64"));
    return parsePythPriceUpdateV2Data(rawBytes);
  }

  async isPythPriceStale(feedId: string, maxAgeSeconds: number): Promise<boolean> {
    const feed = await this.getPythPriceFeed(feedId);
    if (!feed) return true;
    const ageSeconds = Date.now() / 1000 - feed.price.publishTime;
    return ageSeconds > maxAgeSeconds;
  }

  async searchPythFeeds(query: string, assetType?: string): Promise<Array<PythFeedInfo>> {
    const params = new URLSearchParams({ query });
    if (assetType) {
      params.set("asset_type", assetType);
    }
    const response = await fetch(`${this.hermesUrl}/v2/price_feeds?${params}`);
    if (!response.ok) {
      throw new Error(`Hermes API error: ${response.status} ${response.statusText}`);
    }
    const data = (await response.json()) as Array<HermesFeedInfo>;
    return data.map((feed) => ({
      id: normalizeFeedId(feed.id),
      attributes: feed.attributes,
    }));
  }

  watchPythPriceFeed(feedId: string, callback: PythPriceCallback, intervalMs: number = 1000): () => void {
    const poll = async () => {
      let feed: PythPriceFeed | null = null;
      let fetchError: Error | null = null;
      try {
        feed = await this.getPythPriceFeed(feedId);
      } catch (thrownObject) {
        fetchError = ensureError(thrownObject);
      }
      // The callback runs outside the try/catch above so an exception thrown
      // by the caller's callback is not mistaken for a fetch error and fed
      // back into the callback a second time.
      if (fetchError) {
        callback(fetchError, null);
      } else if (feed) {
        callback(null, feed);
      } else {
        callback(new Error(`Price not found for feed ${feedId}`), null);
      }
    };

    void poll();
    const intervalId = setInterval(poll, intervalMs);
    return () => clearInterval(intervalId);
  }

  async postPythPriceUpdate(feedId: string, payer: TransactionSendingSigner): Promise<Address> {
    const [address] = await this.postPythPriceUpdates([feedId], payer);
    return address!;
  }

  async postPythPriceUpdates(feedIds: Array<string>, payer: TransactionSendingSigner): Promise<Array<Address>> {
    const normalizedIds = feedIds.map(normalizeFeedId);

    const { binaryData } = await this.fetchHermesLatest(normalizedIds);
    if (!binaryData) {
      throw new Error("No binary data returned from Hermes");
    }

    const accumulatorUpdate = parseAccumulatorUpdate(binaryData);

    // The guardian set index is at bytes 1-4 of the VAA (big-endian)
    const vaaGuardianSetIndex = new DataView(accumulatorUpdate.vaa.buffer).getUint32(1, false);

    const [configAddress] = await getProgramDerivedAddress({
      programAddress: PYTH_RECEIVER_PROGRAM_ID,
      seeds: [new TextEncoder().encode("config")],
    });

    const [treasuryAddress] = await getProgramDerivedAddress({
      programAddress: PYTH_RECEIVER_PROGRAM_ID,
      seeds: [new TextEncoder().encode("treasury"), new Uint8Array([DEFAULT_TREASURY_ID])],
    });

    const guardianSetIndexBytes = new Uint8Array(4);
    new DataView(guardianSetIndexBytes.buffer).setUint32(0, vaaGuardianSetIndex, false);
    const [guardianSetAddress] = await getProgramDerivedAddress({
      programAddress: WORMHOLE_PROGRAM_ID,
      seeds: [new TextEncoder().encode("GuardianSet"), guardianSetIndexBytes],
    });

    // Send one transaction per feed, all in parallel
    const priceUpdateAddresses = await Promise.all(
      normalizedIds.map(async (feedId) => {
        const update = accumulatorUpdate.updatesByFeedId.get(feedId);
        if (!update) {
          throw new Error(`Feed ${feedId} not found in accumulator update`);
        }

        const priceUpdateSigner = await generateKeyPairSigner();

        const instructionData = buildPostUpdateAtomicData(
          accumulatorUpdate.vaa,
          update.message,
          update.proof,
          DEFAULT_TREASURY_ID,
        );

        const instruction = addSignersToInstruction([payer, priceUpdateSigner], {
          programAddress: PYTH_RECEIVER_PROGRAM_ID,
          accounts: [
            { address: payer.address, role: AccountRole.WRITABLE_SIGNER },
            { address: guardianSetAddress, role: AccountRole.READONLY },
            { address: configAddress, role: AccountRole.READONLY },
            { address: treasuryAddress, role: AccountRole.WRITABLE },
            { address: priceUpdateSigner.address, role: AccountRole.WRITABLE_SIGNER },
            { address: SYSTEM_PROGRAM_ADDRESS, role: AccountRole.READONLY },
            { address: payer.address, role: AccountRole.READONLY_SIGNER },
          ],
          data: instructionData,
        });

        await this.connection.sendTransactionFromInstructions({
          feePayer: payer,
          instructions: [instruction],
        });

        return priceUpdateSigner.address;
      }),
    );

    return priceUpdateAddresses;
  }

  async reclaimPythPriceUpdateRent(priceUpdateAccount: Address, payer: TransactionSendingSigner): Promise<string> {
    const instruction = addSignersToInstruction([payer], {
      programAddress: PYTH_RECEIVER_PROGRAM_ID,
      accounts: [
        { address: payer.address, role: AccountRole.WRITABLE_SIGNER },
        { address: priceUpdateAccount, role: AccountRole.WRITABLE },
      ],
      data: RECLAIM_RENT_DISCRIMINATOR,
    });

    return this.connection.sendTransactionFromInstructions({
      feePayer: payer,
      instructions: [instruction],
    });
  }
}
