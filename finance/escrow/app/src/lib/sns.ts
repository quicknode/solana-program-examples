import { type Address, createSolanaRpc } from "@solana/kit";
import { getPrimaryDomainsBatch } from "@solana-name-service/sns-sdk-kit";
import { SNS_RPC_URL } from "./config";

// A wallet's primary .sol name from the Solana Name Service, the name its
// owner chose to be shown as, or null if it has none. The SNS SDK returns a
// name only while the wallet still owns the domain, so a sold or transferred
// domain's old primary is not shown.
let snsRpc: ReturnType<typeof createSolanaRpc> | undefined;

export const lookupSolName = async (owner: Address): Promise<string | null> => {
  snsRpc ??= createSolanaRpc(SNS_RPC_URL);
  const [name] = await getPrimaryDomainsBatch({ rpc: snsRpc, walletAddresses: [owner] });
  return name ? `${name}.sol` : null;
};
