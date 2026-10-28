import { isNativeSol, type MintInfo } from "./tokens";
import type { WalletToken } from "./wallet-tokens";

// SOL kept back for transaction fees and the rent deposits of any accounts a
// transaction creates, so the app never offers or spends a wallet's last
// lamport. 0.01 SOL covers several fees and token accounts.
export const SOL_RESERVE_LAMPORTS = 10_000_000n;

// How much of a token a wallet can put into a trade. For most tokens that is
// what its token account holds. For SOL it is the wallet's SOL, less the
// reserve, plus anything already wrapped in its native-mint account: the app
// wraps SOL as a transaction needs it.
export const spendableBalance = (mint: MintInfo, walletTokens: Array<WalletToken>, lamports: bigint): bigint => {
  const held = walletTokens.find((token) => token.mint.address === mint.address)?.amount ?? 0n;
  if (!isNativeSol(mint)) {
    return held;
  }
  const unwrapped = lamports > SOL_RESERVE_LAMPORTS ? lamports - SOL_RESERVE_LAMPORTS : 0n;
  return held + unwrapped;
};

// What the wallet's native-mint account holds now, which a wrap tops up
// rather than replaces.
export const wrappedSolBalance = (walletTokens: Array<WalletToken>): bigint =>
  walletTokens.find((token) => isNativeSol(token.mint))?.amount ?? 0n;
