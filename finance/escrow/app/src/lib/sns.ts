import { type Address, createSolanaRpc } from "@solana/kit";
import { getPrimaryDomainsBatch } from "@solana-name-service/sns-sdk-kit";
import { SNS_RPC_URL } from "./config";

// A wallet's primary name from the Solana Name Service, the name its owner
// chose to be shown as, or null if it has none. The SDK returns the name
// without its suffix, so this adds .sns, the suffix the SDK's README gives
// for every SNS name. The SDK returns a name only while the wallet still owns
// the domain, so a sold or transferred domain's old primary is not shown.
let snsRpc: ReturnType<typeof createSolanaRpc> | undefined;

export const lookupSnsName = async (owner: Address): Promise<string | null> => {
  snsRpc ??= createSolanaRpc(SNS_RPC_URL);
  const [name] = await getPrimaryDomainsBatch({ rpc: snsRpc, walletAddresses: [owner] });
  return name ? `${name}.sns` : null;
};
