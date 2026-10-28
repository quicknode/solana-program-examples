import {
  type Address,
  type Base64EncodedDataResponse,
  type GetMultipleAccountsApi,
  getBase64Encoder,
  type Rpc,
  unwrapOption,
} from "@solana/kit";
import { TOKEN_PROGRAM_ADDRESS } from "@solana-program/token";
import { getMintDecoder, getTokenDecoder, TOKEN_2022_PROGRAM_ADDRESS } from "@solana-program/token-2022";
import { NATIVE_MINT_ADDRESS, POPULAR_TOKENS } from "@/components/token-picker/popular-token-addresses";
import { fetchAccounts, type RawAccount } from "./accounts";

// What the app knows about a mint: which token program owns it (the escrow
// program accepts both), its decimals, and a name to show people.
export type MintInfo = {
  address: Address;
  tokenProgram: Address;
  decimals: number;
  // Who may mint more of this token, or null for a fixed supply.
  mintAuthority: Address | null;
  symbol: string;
  name: string;
  // True when the symbol came from the mint itself or from the list below,
  // false when it is a shortened address standing in for one.
  isNamed: boolean;
};

// Mints whose names the app knows without reading any metadata: every token
// in the popular-token list, on every cluster. A Token-2022 mint can carry its
// own name and symbol, which then wins; a classic token program mint keeps its
// name in a separate metadata program the app does not read. Each address,
// and the page that proves it, is in popular-token-addresses.ts.
const KNOWN_MINTS: Record<string, { symbol: string; name: string }> = Object.fromEntries(
  Object.values(POPULAR_TOKENS)
    .flat()
    .map((token) => [token.address, { symbol: token.symbol, name: token.name }]),
);

// Whether a mint is the native mint, whose tokens are SOL held in a token
// account. The app shows them as SOL and wraps and unwraps them for the user.
export const isNativeSol = (mint: { address: string } | string): boolean =>
  (typeof mint === "string" ? mint : mint.address) === NATIVE_MINT_ADDRESS;

export const shortAddress = (value: string): string => `${value.slice(0, 4)}…${value.slice(-4)}`;

const decodeBase64 = (data: Base64EncodedDataResponse): Uint8Array => getBase64Encoder().encode(data[0]) as Uint8Array;

// Decode a mint account fetched with base64 encoding. Token-2022's decoder
// reads classic token program mints too: a classic mint is a Token-2022 mint
// with no extensions.
export const decodeMintInfo = (mintAddress: Address, account: RawAccount): MintInfo | null => {
  if (account === null) {
    return null;
  }
  if (account.owner !== TOKEN_PROGRAM_ADDRESS && account.owner !== TOKEN_2022_PROGRAM_ADDRESS) {
    return null;
  }
  const mint = getMintDecoder().decode(decodeBase64(account.data));
  const known = KNOWN_MINTS[mintAddress];
  let symbol = known?.symbol;
  let name = known?.name;
  const extensions = unwrapOption(mint.extensions) ?? [];
  for (const extension of extensions) {
    if (extension.__kind === "TokenMetadata" && extension.symbol !== "") {
      symbol = extension.symbol;
      name = extension.name;
    }
  }
  return {
    address: mintAddress,
    tokenProgram: account.owner,
    decimals: mint.decimals,
    mintAuthority: unwrapOption(mint.mintAuthority),
    symbol: symbol ?? shortAddress(mintAddress),
    name: name ?? "Unnamed token",
    isNamed: symbol !== undefined,
  };
};

// The amount held by a token account fetched with base64 encoding, or null if
// the account does not exist.
export const decodeTokenAmount = (account: RawAccount): bigint | null => {
  if (account === null) {
    return null;
  }
  return getTokenDecoder().decode(decodeBase64(account.data)).amount;
};

// One mint's details, or null if the address holds no mint.
export const fetchMintInfo = async (
  rpc: Rpc<GetMultipleAccountsApi>,
  mintAddress: Address,
  abortSignal?: AbortSignal,
): Promise<MintInfo | null> => {
  const accounts = await fetchAccounts(rpc, [mintAddress], abortSignal);
  return decodeMintInfo(mintAddress, accounts.get(mintAddress) ?? null);
};
