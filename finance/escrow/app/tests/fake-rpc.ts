import {
  type Address,
  type Base64EncodedDataResponse,
  getAddressDecoder,
  getBase58Encoder,
  getBase64Decoder,
  type ReadonlyUint8Array,
} from "@solana/kit";

// An address made of 32 copies of one byte, for tests that need many distinct
// addresses and care about none of them.
export const testAddress = (byte: number): Address => getAddressDecoder().decode(new Uint8Array(32).fill(byte));

// A stand-in for an RPC node, holding accounts in memory and answering the
// handful of methods the app reads with. Each method returns an object with a
// `send` method, the shape Kit's RPC uses.
export type FakeAccount = { owner: Address; data: ReadonlyUint8Array };

export const createFakeRpc = (accounts: Map<Address, FakeAccount>) => {
  const toResponse = (account: FakeAccount | undefined) =>
    account === undefined
      ? null
      : {
          owner: account.owner,
          data: [getBase64Decoder().decode(account.data), "base64"] as Base64EncodedDataResponse,
          executable: false,
          lamports: 1_000_000n,
          space: BigInt(account.data.length),
        };
  const sendable = <T>(value: T) => ({ send: async (_options?: { abortSignal?: AbortSignal }) => value });

  type Filter = { dataSize?: bigint; memcmp?: { offset: bigint; bytes: string } };

  return {
    getAccountInfo: (address: Address) => sendable({ context: { slot: 1n }, value: toResponse(accounts.get(address)) }),
    getMultipleAccounts: (addresses: Array<Address>) =>
      sendable({ context: { slot: 1n }, value: addresses.map((address) => toResponse(accounts.get(address))) }),
    getProgramAccounts: (programAddress: Address, config: { filters: Array<Filter> }) => {
      const matches = [...accounts.entries()].filter(([, account]) => {
        if (account.owner !== programAddress) {
          return false;
        }
        return config.filters.every((filter) => {
          if (filter.dataSize !== undefined) {
            return BigInt(account.data.length) === filter.dataSize;
          }
          if (filter.memcmp) {
            const expected = getBase58Encoder().encode(filter.memcmp.bytes);
            const offset = Number(filter.memcmp.offset);
            return expected.every((byte, index) => account.data[offset + index] === byte);
          }
          return true;
        });
      });
      return sendable(matches.map(([pubkey, account]) => ({ pubkey, account: toResponse(account) })));
    },
  };
};
