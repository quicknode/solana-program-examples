import type { Address, GetMultipleAccountsApi, GetTokenAccountsByOwnerApi, Rpc } from "@solana/kit";
import { findAssociatedTokenPda, TOKEN_PROGRAM_ADDRESS } from "@solana-program/token";
import { TOKEN_2022_PROGRAM_ADDRESS } from "@solana-program/token-2022";
import { fetchAccounts } from "./accounts";
import { decodeMintInfo, type MintInfo } from "./tokens";

export type WalletToken = {
  mint: MintInfo;
  // The wallet's associated token account for this mint, the only token
  // account the escrow program moves a wallet's tokens in and out of.
  tokenAccount: Address;
  amount: bigint;
};

// The tokens a wallet holds in its associated token accounts, under either
// token program: tokens with a known name first, then by symbol.
export const fetchWalletTokens = async (
  rpc: Rpc<GetTokenAccountsByOwnerApi & GetMultipleAccountsApi>,
  owner: Address,
  abortSignal?: AbortSignal,
): Promise<Array<WalletToken>> => {
  const responses = await Promise.all(
    [TOKEN_PROGRAM_ADDRESS, TOKEN_2022_PROGRAM_ADDRESS].map((programId) =>
      rpc
        .getTokenAccountsByOwner(owner, { programId }, { encoding: "jsonParsed", commitment: "confirmed" })
        .send({ abortSignal }),
    ),
  );
  const holdings = responses.flatMap(({ value }) =>
    value.map(({ pubkey, account }) => ({
      tokenAccount: pubkey,
      mint: account.data.parsed.info.mint,
      amount: BigInt(account.data.parsed.info.tokenAmount.amount),
    })),
  );

  const mintAccounts = await fetchAccounts(rpc, [...new Set(holdings.map(({ mint }) => mint))], abortSignal);

  const tokens = await Promise.all(
    holdings.map(async (holding): Promise<WalletToken | null> => {
      const mint = decodeMintInfo(holding.mint, mintAccounts.get(holding.mint) ?? null);
      if (!mint) {
        return null;
      }
      const [associatedTokenAccount] = await findAssociatedTokenPda({
        owner,
        mint: mint.address,
        tokenProgram: mint.tokenProgram,
      });
      if (associatedTokenAccount !== holding.tokenAccount) {
        return null;
      }
      return { mint, tokenAccount: holding.tokenAccount, amount: holding.amount };
    }),
  );

  return tokens
    .filter((token): token is WalletToken => token !== null)
    .sort((first, second) => {
      if (first.mint.isNamed !== second.mint.isNamed) {
        return first.mint.isNamed ? -1 : 1;
      }
      return first.mint.symbol.localeCompare(second.mint.symbol);
    });
};
