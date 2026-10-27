import { createClient } from "@solana/kit";
import { rpcAirdrop, solanaRpc } from "@solana/kit-plugin-rpc";
import { walletSigner } from "@solana/kit-plugin-wallet";
import { tokenProgram } from "@solana-program/token";
import { token2022Program } from "@solana-program/token-2022";
import { escrowProgram } from "../generated";
import { ESCROW_PROGRAM, RPC_SUBSCRIPTIONS_URL, RPC_URL, WALLET_CHAIN } from "./config";

// One Kit client for the whole app, assembled from plugins:
// - walletSigner: finds Wallet Standard wallets, connects one, and makes the
//   connected account the client's payer and identity.
// - solanaRpc: the RPC connection, plus planning and sending transactions,
//   paid for by that payer.
// - rpcAirdrop: SOL from the faucet, on the test clusters that have one.
// - escrowProgram: the escrow program's instructions, accounts and PDAs, from
//   the Codama client generated from the program's IDL.
// - tokenProgram, token2022Program: the two token programs, used for the test
//   tokens a user can mint on a test cluster.
export const createEscrowClient = () =>
  createClient()
    .use(walletSigner({ chain: WALLET_CHAIN, storageKey: "escrow-app:wallet" }))
    .use(
      solanaRpc({
        rpcUrl: RPC_URL,
        rpcSubscriptionsUrl: RPC_SUBSCRIPTIONS_URL,
        // Version 0 rather than the plugin's default of version 1, because
        // every wallet signs version 0 and not every wallet signs version 1
        // yet. The planner still simulates each transaction to set its compute
        // unit limit.
        transactionConfig: { version: 0 },
      }),
    )
    .use(rpcAirdrop())
    .use(escrowProgram({ programAddress: ESCROW_PROGRAM }))
    .use(tokenProgram())
    .use(token2022Program());

export type EscrowClient = ReturnType<typeof createEscrowClient>;
