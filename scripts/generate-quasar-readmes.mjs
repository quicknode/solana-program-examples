#!/usr/bin/env node
/**
 * Generate Quasar README.md files. Run: node scripts/generate-quasar-readmes.mjs
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");

/** @type {Record<string, { title: string; purpose: string; concepts: string[] }>} */
const examples = {
  "basics/account-data/quasar": {
    title: "Account Data",
    purpose:
      "Store and retrieve data in a [program](https://solana.com/docs/terminology#program)-owned [account](https://solana.com/docs/terminology#account).",
    concepts: ["Account layout and serialization", "Quasar account views"],
  },
  "basics/checking-accounts/quasar": {
    title: "Checking Accounts",
    purpose:
      "Validate signers, owners, and addresses on incoming [instructions](https://solana.com/docs/terminology#instruction).",
    concepts: ["Compile-time account checks", "Signer and mut constraints"],
  },
  "basics/close-account/quasar": {
    title: "Close Account",
    purpose:
      "Create a PDA [account](https://solana.com/docs/terminology#account), then close it and return [rent](https://solana.com/docs/terminology#rent) to the user.",
    concepts: ["PDA init and close", "Rent reclamation"],
  },
  "basics/counter/quasar": {
    title: "Counter",
    purpose:
      "Global counter in a [PDA](https://solana.com/docs/terminology#program-derived-address-pda) with initialize and increment handlers.",
    concepts: ["PDA state", "Handler dispatch"],
  },
  "basics/create-account/quasar": {
    title: "Create Account",
    purpose: "Create and fund rent-exempt accounts via the System Program.",
    concepts: ["System Program CPI", "Rent-exempt lamports"],
  },
  "basics/favorites/quasar": {
    title: "Favorites",
    purpose: "Per-user favorites in a PDA; only the owner can update their data.",
    concepts: ["Per-user PDA", "Authority checks"],
  },
  "basics/hello-solana/quasar": {
    title: "Hello Solana",
    purpose: "Minimal program that logs a greeting.",
    concepts: ["Program entrypoint", "Instruction data"],
  },
  "basics/pda-rent-payer/quasar": {
    title: "PDA Rent Payer",
    purpose:
      "A [PDA](https://solana.com/docs/terminology#program-derived-address-pda) pays [rent](https://solana.com/docs/terminology#rent) when creating another account.",
    concepts: ["PDA signer", "Rent payer pattern"],
  },
  "basics/processing-instructions/quasar": {
    title: "Processing Instructions",
    purpose: "Pass arguments into an [instruction handler](https://solana.com/docs/terminology#instruction-handler).",
    concepts: ["Instruction data", "Handler parameters"],
  },
  "basics/program-derived-addresses/quasar": {
    title: "Program Derived Addresses",
    purpose: "Derive and use PDAs for deterministic program-owned addresses.",
    concepts: ["Seed derivation", "PDA-owned state"],
  },
  "basics/pyth/quasar": {
    title: "Pyth Price Feeds",
    purpose: "Read a Pyth price feed and use oracle data in program logic.",
    concepts: ["Oracle accounts", "Price feed layout"],
  },
  "basics/realloc/quasar": {
    title: "Realloc",
    purpose: "Grow or shrink account data when storage needs change.",
    concepts: ["Account reallocation", "Rent on resize"],
  },
  "basics/rent/quasar": {
    title: "Rent",
    purpose: "Compute account size and minimum rent-exempt [lamports](https://solana.com/docs/terminology#lamport).",
    concepts: ["Rent-exempt balance", "Space planning"],
  },
  "basics/repository-layout/quasar": {
    title: "Repository Layout",
    purpose: "Organize a program across modules (state, handlers, errors).",
    concepts: ["Multi-file layout", "Separation of concerns"],
  },
  "basics/transfer-sol/quasar": {
    title: "Transfer SOL",
    purpose: "Transfer native SOL via the System Program.",
    concepts: ["System transfer CPI", "Signer-funded lamports"],
  },
  "compression/cnft-burn/quasar": {
    title: "cNFT Burn",
    purpose: "Burn compressed NFTs via Metaplex Bubblegum CPIs.",
    concepts: ["Compressed NFTs", "Bubblegum CPI"],
  },
  "compression/cnft-vault/quasar": {
    title: "cNFT Vault",
    purpose: "Deposit and withdraw compressed NFTs from a PDA vault.",
    concepts: ["cNFT transfers", "PDA vault"],
  },
  "compression/cutils/quasar": {
    title: "Compression Utilities",
    purpose: "Helpers for working with Metaplex compressed NFTs in a program.",
    concepts: ["Compression proofs", "Merkle tree accounts"],
  },
  "finance/escrow/quasar": {
    title: "Escrow",
    purpose: "Atomic token swap escrow between maker and taker.",
    concepts: ["Escrow PDA", "See [Anchor variant](../anchor/README.md) for the full walkthrough"],
  },
  "finance/token-fundraiser/quasar": {
    title: "Token Fundraiser",
    purpose: "Onchain crowdfunding toward a target amount in a chosen token.",
    concepts: ["Fundraiser PDA", "Contributor deposits"],
  },
  "finance/token-swap/quasar": {
    title: "Token Swap (AMM)",
    purpose: "Constant-product AMM: pools, liquidity, swaps with slippage guards.",
    concepts: ["Pool PDA and LP tokens", "See [finance/token-swap/README.md](../token-swap/README.md)"],
  },
  "tokens/create-token/quasar": {
    title: "Create Token",
    purpose: "Create a mint with metadata using Token and Metaplex programs.",
    concepts: ["Mint + metadata CPI", "See [tokens/create-token/README.md](../create-token/README.md)"],
  },
  "tokens/external-delegate-token-master/quasar": {
    title: "External Delegate Token Master",
    purpose: "Token transfers authorized by an external secp256k1 signature.",
    concepts: ["Delegate approval", "Signature verification"],
  },
  "tokens/nft-minter/quasar": {
    title: "NFT Minter",
    purpose: "Mint an NFT from inside your program.",
    concepts: ["NFT mint", "Metadata CPI"],
  },
  "tokens/nft-operations/quasar": {
    title: "NFT Operations",
    purpose: "Collection mint, NFT mint, and collection verification via Metaplex.",
    concepts: ["Collection NFTs", "Verification CPI"],
  },
  "tokens/pda-mint-authority/quasar": {
    title: "PDA Mint Authority",
    purpose: "Mint with a PDA as mint authority.",
    concepts: ["PDA mint authority", "mint_to CPI"],
  },
  "tokens/token-minter/quasar": {
    title: "Token Minter",
    purpose: "Mint tokens using the [Classic Token Program](https://solana.com/docs/terminology#token-program).",
    concepts: ["Mint authority", "Token account init"],
  },
  "tokens/transfer-tokens/quasar": {
    title: "Transfer Tokens",
    purpose: "Transfer tokens between accounts via CPI.",
    concepts: ["Token transfer CPI", "Associated token accounts"],
  },
  "tokens/token-extensions/basics/quasar": {
    title: "Token Extensions — Basics",
    purpose:
      "Mint and transfer with the [Token Extensions Program](https://solana.com/docs/terminology#token-extensions-program).",
    concepts: ["Extension mints", "Token Extensions CPI"],
  },
  "tokens/token-extensions/cpi-guard/quasar": {
    title: "Token Extensions — CPI Guard",
    purpose: "Block certain token actions inside CPI contexts.",
    concepts: ["CPI Guard extension"],
  },
  "tokens/token-extensions/default-account-state/quasar": {
    title: "Token Extensions — Default Account State",
    purpose: "New token accounts frozen by default until thawed.",
    concepts: ["Default account state extension"],
  },
  "tokens/token-extensions/group/quasar": {
    title: "Token Extensions — Group Pointer",
    purpose: "Link mints to a group via Group Pointer.",
    concepts: ["Group pointer extension"],
  },
  "tokens/token-extensions/immutable-owner/quasar": {
    title: "Token Extensions — Immutable Owner",
    purpose: "Token accounts with an immutable owner field.",
    concepts: ["Immutable owner extension"],
  },
  "tokens/token-extensions/interest-bearing/quasar": {
    title: "Token Extensions — Interest Bearing",
    purpose: "Balances that reflect accrued interest over time.",
    concepts: ["Interest bearing extension"],
  },
  "tokens/token-extensions/memo-transfer/quasar": {
    title: "Token Extensions — Memo Transfer",
    purpose: "Require a memo on every transfer.",
    concepts: ["Memo transfer extension"],
  },
  "tokens/token-extensions/mint-close-authority/quasar": {
    title: "Token Extensions — Mint Close Authority",
    purpose: "Designated account may close the mint.",
    concepts: ["Mint close authority extension"],
  },
  "tokens/token-extensions/non-transferable/quasar": {
    title: "Token Extensions — Non-Transferable",
    purpose: "Tokens that cannot be transferred.",
    concepts: ["Non-transferable extension"],
  },
  "tokens/token-extensions/permanent-delegate/quasar": {
    title: "Token Extensions — Permanent Delegate",
    purpose: "Permanent delegate retains transfer rights.",
    concepts: ["Permanent delegate extension"],
  },
  "tokens/token-extensions/transfer-fee/quasar": {
    title: "Token Extensions — Transfer Fee",
    purpose: "Fee charged on each transfer at the mint.",
    concepts: ["Transfer fee extension"],
  },
  "tokens/token-extensions/transfer-hook/account-data-as-seed/quasar": {
    title: "Transfer Hook — Account Data as Seed",
    purpose: "Derive extra accounts from token account data in a transfer hook.",
    concepts: ["Transfer hook", "Extra account metas"],
  },
  "tokens/token-extensions/transfer-hook/allow-block-list-token/quasar": {
    title: "Transfer Hook — Allow/Block List",
    purpose: "Allow/block list enforced by a transfer hook program.",
    concepts: ["Transfer hook", "List authority"],
  },
  "tokens/token-extensions/transfer-hook/counter/quasar": {
    title: "Transfer Hook — Counter",
    purpose: "Count transfers in hook-side state.",
    concepts: ["Transfer hook", "Counter PDA"],
  },
  "tokens/token-extensions/transfer-hook/hello-world/quasar": {
    title: "Transfer Hook — Hello World",
    purpose: "Minimal transfer hook executed on each transfer.",
    concepts: ["Transfer hook", "Extra account meta list"],
  },
  "tokens/token-extensions/transfer-hook/transfer-cost/quasar": {
    title: "Transfer Hook — Transfer Cost",
    purpose: "Additional fee on each transfer via the hook.",
    concepts: ["Transfer hook", "Fee collection"],
  },
  "tokens/token-extensions/transfer-hook/transfer-switch/quasar": {
    title: "Transfer Hook — Transfer Switch",
    purpose: "Globally enable or disable transfers.",
    concepts: ["Transfer hook", "Admin switch"],
  },
  "tokens/token-extensions/transfer-hook/whitelist/quasar": {
    title: "Transfer Hook — Whitelist",
    purpose: "Only whitelisted accounts may receive tokens.",
    concepts: ["Transfer hook", "Whitelist PDA"],
  },
};

const redirects = {
  "tokens/token-2022/default-account-state/quasar": "tokens/token-extensions/default-account-state/quasar",
  "tokens/token-2022/metadata/quasar": "tokens/token-extensions/metadata/anchor",
};

function titleCase(segment) {
  return segment
    .split("-")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

function parentOverviewRel(quasarRel) {
  const parent = path.dirname(quasarRel);
  const readme = path.join(root, parent, "README.md");
  if (fs.existsSync(readme)) {
    return `See also: [${titleCase(path.basename(parent))} overview](../README.md) and the [repository catalog](${"../".repeat(quasarRel.split("/").length)}README.md).`;
  }
  const depth = quasarRel.split("/").length;
  return `See also: the [repository catalog](${"../".repeat(depth)}README.md).`;
}

function render(quasarRel, meta) {
  const concepts = meta.concepts.map((c) => `- ${c}`).join("\n");
  return `# ${meta.title} (Quasar)

${meta.purpose}

${parentOverviewRel(quasarRel)}

## Major concepts

${concepts}

## Setup

From \`${quasarRel}/\`:

\`\`\`bash
quasar build
\`\`\`

Prerequisites: [Quasar](https://quasar-lang.com/docs) CLI and [Agave](https://docs.anza.xyz/) toolchain (see \`Quasar.toml\`).

## Testing

In-process tests via **Quasar SVM** (\`quasar-svm\` in \`Quasar.toml\`):

\`\`\`bash
cargo test
\`\`\`

Tests invoke instruction handlers and assert onchain state. No local validator.

## Usage

Read \`src/\` and \`Quasar.toml\`. Compare with the [Anchor](../anchor/) variant in the same example where present.
`;
}

function renderRedirect(fromRel, toRel) {
  const fromDir = path.join(root, fromRel);
  const toReadme = path.join(root, toRel, "README.md");
  let link = path.relative(fromDir, toReadme).replace(/\\/g, "/");
  if (!link.startsWith(".")) link = `./${link}`;
  return `# Deprecated path

This tree is a leftover \`token-2022\` path. Use [\`${toRel}\`](${link}) instead.
`;
}

let written = 0;
for (const [rel, meta] of Object.entries(examples)) {
  const readmePath = path.join(root, rel, "README.md");
  if (fs.existsSync(readmePath)) continue;
  fs.mkdirSync(path.dirname(readmePath), { recursive: true });
  fs.writeFileSync(readmePath, render(rel, meta));
  written++;
  console.log("wrote", rel);
}

for (const [rel, to] of Object.entries(redirects)) {
  const readmePath = path.join(root, rel, "README.md");
  if (fs.existsSync(readmePath)) continue;
  fs.mkdirSync(path.dirname(readmePath), { recursive: true });
  fs.writeFileSync(readmePath, renderRedirect(rel, to));
  written++;
  console.log("redirect", rel);
}

console.log(`Done. ${written} README(s) created.`);
