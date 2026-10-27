import {
  type Address,
  type ClientWithGetMinimumBalance,
  generateKeyPairSigner,
  type InstructionPlan,
  none,
  sequentialInstructionPlan,
  some,
  type TransactionSigner,
} from "@solana/kit";
import { getCreateMintInstructionPlan, getMintToATAInstructionPlanAsync } from "@solana-program/token-2022";
import type { MintInfo } from "./tokens";

// Test tokens for trying the escrow on a test cluster. They are Token-2022
// mints carrying their own name and symbol (the metadata pointer and token
// metadata extensions), so every visitor sees them by name, and the wallet
// that creates them is their mint authority and can mint more to anyone.

export type TestTokenSpec = { name: string; symbol: string; decimals: number; initialAmount: bigint };

// The book's story: Alice offers 250 USDC for 1 TSLAx. These are stand-ins,
// named so nobody mistakes them for the real tokens.
export const TEST_TOKENS: Array<TestTokenSpec> = [
  { name: "Test USDC", symbol: "tUSDC", decimals: 6, initialAmount: 1_000n * 10n ** 6n },
  { name: "Test TSLAx", symbol: "tTSLAx", decimals: 8, initialAmount: 10n * 10n ** 8n },
];

export const buildCreateTestTokenPlan = async (
  client: ClientWithGetMinimumBalance,
  authority: TransactionSigner,
  spec: TestTokenSpec,
): Promise<{ mint: Address; plan: InstructionPlan }> => {
  const newMint = await generateKeyPairSigner();
  const createMint = await getCreateMintInstructionPlan(client, {
    payer: authority,
    newMint,
    decimals: spec.decimals,
    mintAuthority: authority,
    freezeAuthority: none(),
    extensions: [
      { __kind: "MetadataPointer", authority: some(authority.address), metadataAddress: some(newMint.address) },
      {
        __kind: "TokenMetadata",
        updateAuthority: some(authority.address),
        mint: newMint.address,
        name: spec.name,
        symbol: spec.symbol,
        uri: "",
        additionalMetadata: new Map(),
      },
    ],
  });
  const mintToCreator = await getMintToATAInstructionPlanAsync({
    payer: authority,
    owner: authority.address,
    mint: newMint.address,
    mintAuthority: authority,
    amount: spec.initialAmount,
    decimals: spec.decimals,
  });
  return { mint: newMint.address, plan: sequentialInstructionPlan([createMint, mintToCreator]) };
};

// Mint more of a test token to any wallet, creating that wallet's token
// account for it if needed. The authority pays for that account.
export const buildMintTestTokenPlan = (
  authority: TransactionSigner,
  mint: MintInfo,
  recipient: Address,
  amount: bigint,
): Promise<InstructionPlan> =>
  getMintToATAInstructionPlanAsync({
    payer: authority,
    owner: recipient,
    mint: mint.address,
    mintAuthority: authority,
    amount,
    decimals: mint.decimals,
  });
