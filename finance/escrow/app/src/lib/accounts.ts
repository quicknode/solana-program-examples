import type { Address, Base64EncodedDataResponse, GetMultipleAccountsApi, Rpc } from "@solana/kit";

// An account as getMultipleAccounts returns it with base64 encoding, reduced
// to the two fields the app reads, or null when no account exists there.
export type RawAccount = { owner: Address; data: Base64EncodedDataResponse } | null;

// getMultipleAccounts takes at most 100 addresses per call.
const MULTIPLE_ACCOUNTS_LIMIT = 100;

export const fetchAccounts = async (
  rpc: Rpc<GetMultipleAccountsApi>,
  addresses: Array<Address>,
  abortSignal?: AbortSignal,
): Promise<Map<Address, RawAccount>> => {
  const accounts = new Map<Address, RawAccount>();
  for (let start = 0; start < addresses.length; start += MULTIPLE_ACCOUNTS_LIMIT) {
    const chunk = addresses.slice(start, start + MULTIPLE_ACCOUNTS_LIMIT);
    const { value } = await rpc
      .getMultipleAccounts(chunk, { encoding: "base64", commitment: "confirmed" })
      .send({ abortSignal });
    chunk.forEach((chunkAddress, index) => {
      accounts.set(chunkAddress, value[index] ?? null);
    });
  }
  return accounts;
};
