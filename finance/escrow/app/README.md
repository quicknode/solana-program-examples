# Solana Escrow (Web App)

A browser frontend for the [escrow program](../anchor/README.md): browse the open offers, pick one and take it, make your own, and cancel it. Every screen explains what the program is doing, in the words and the look of [Building Financial Software on Solana](https://solanabook.org), whose Escrow chapter describes the same program.

The app talks to the Anchor build of the program, in [`../anchor`](../anchor/) (Anchor v2) or [`../anchor-v1`](../anchor-v1/) (Anchor v1). Both have the same instruction handlers, arguments, accounts and account layout, so one client serves both. The [Quasar](../quasar/) and [native](../native/) builds lay their instructions and accounts out differently and need clients of their own.

## What it does

- **Offers** (`/`): every open offer, read from the chain. Filter by the token you want and the token you can pay with, sort by price, and see on each card whether your wallet can pay for it. New offers appear as they are made, over an RPC subscription to the program's accounts.
- **An offer** (`/offers/<address>`): what you pay, what you get, and the price both ways. A checklist of your balances, the balances before and after taking, a step-by-step list of what `take_offer` moves, and every account behind the offer with the reason it exists. The maker sees a cancel panel instead.
- **Make an offer** (`/make`): pick a token you hold and an amount, a token you want and an amount, check the price and the rent deposit, and send `make_offer`. The token you want is one click: the cluster's popular tokens come first (see below), then anything already traded or in your wallet, then **Other…** for any mint address. A token that can't be paired with the one you give is shown greyed out, with the reason: the program moves both tokens of an offer with one token program, so a classic token program token like USDC can't be swapped for a Token-2022 token like PYUSD.
- **Test tokens** (`/test-tokens`, test clusters only): test SOL from the faucet, two Token-2022 test tokens (tUSDC and tTSLAx, named on the mint itself so every visitor sees their names), and minting more of them to any address, so one person can try both sides of a trade with two wallet accounts.
- **How it works** (`/how-it-works`): the program in plain language: the three handlers, the offer and vault accounts, why a take carries the taker's terms, who pays rent, and what the program refuses.

The wallet button shows the connected wallet's primary [SNS](https://sns.id) name, such as `bob.sns`, in place of its address when it has one. SNS names live on mainnet, so the lookup always reads a mainnet RPC (see `NEXT_PUBLIC_SNS_RPC_URL`), whichever cluster the app trades on.

### Popular tokens

[`src/components/token-picker/popular-token-addresses.ts`](src/components/token-picker/popular-token-addresses.ts) lists the tokens offered in one click on each cluster. Every address in it has a comment above it naming the issuer-controlled page that publishes it, and the network that page lists it under; an address nobody can prove from such a page does not go in.

- **Devnet:** SOL, USDC, EURC, PYUSD, USDG, PAXG and JitoSOL, the mainstream assets whose issuers publish devnet mints. No issuer publishes devnet mints of tokenized stocks, so there are none.
- **Mainnet:** SOL, USDC, EURC, PYUSD, JitoSOL and cbBTC.

The app reads each listed address when it loads and offers only those that are mints on the connected cluster. `pnpm check-popular-tokens --cluster devnet` (or `mainnet`, optionally with `--rpc <url>`) makes the same check from the command line, prints each token's program and decimals, and exits non-zero if any address is not a mint. Run it after changing the list.

Each popular token can have a logo in `public/tokens`, taken from the issuer's own brand assets and named in its entry's `logo` field; a token without one shows the first letter of its symbol. No logos are bundled yet.

### SOL

SOL is offered and paid as SOL. The escrow program moves tokens, and SOL as a token is the native mint: lamports held in a token account. So the app does the wrapping:

- Offering SOL, or paying SOL for an offer, moves the lamports into your native-mint token account first, in the same transaction.
- Receiving SOL from an offer, or cancelling an offer of SOL, closes that account at the end, which returns the SOL, and the account's rent, to your wallet as plain SOL.
- A maker who asked for SOL is paid while they are not there, into their native-mint token account, as wrapped SOL. The Make page and the wallet menu offer to unwrap it with one click. While the maker still has offers asking for SOL, unwrapping leaves an empty account open, because `take_offer` pays into it and requires it to exist.

The app keeps 0.01 SOL of your balance back for fees and rent when working out how much SOL you can offer or pay.

Taking an offer sends the terms you were shown as `take_offer`'s `minimum_token_a_out` and `maximum_token_b_in`, so if the maker cancels and re-makes the offer at worse terms before your transaction lands, the program refuses it and nothing moves.

## Stack

- [Next.js](https://nextjs.org) 16 with the App Router.
- [Solana Kit](https://solanakit.com) 8, composed from Kit plugins in [`src/lib/client.ts`](src/lib/client.ts): `walletSigner` from [`@solana/kit-plugin-wallet`](https://www.npmjs.com/package/@solana/kit-plugin-wallet) (Anza's Wallet Standard connection plugin, which makes the connected wallet the client's payer and signer), `solanaRpc` and `rpcAirdrop` from `@solana/kit-plugin-rpc`, and the `tokenProgram` and `token2022Program` plugins.
- The escrow client in [`src/generated`](src/generated/), generated by [Codama](https://github.com/codama-idl/codama) from the program's IDL. It exports `escrowProgram()`, a Kit plugin that adds the program's instructions, accounts and PDAs to the client.
- [`@solana-name-service/sns-sdk-kit`](https://www.npmjs.com/package/@solana-name-service/sns-sdk-kit), the Solana Name Service's Kit SDK, for `.sns` names. It asks for `@solana-program/token` 0.14, which needs a function Kit 8 removed; `pnpm-workspace.yaml` gives it 0.18, which has everything the SDK uses.
- [`@solana/react`](https://www.npmjs.com/package/@solana/react) for the client provider, data requests (`useRequest`), tracked actions (`useAction`), and the airdrop hook, and `@solana/kit-plugin-wallet/react` for the wallet hooks.
- Plain CSS in [`src/app/globals.css`](src/app/globals.css): named classes, CSS grid and flexbox, and no CSS framework.

### Typography and color

The app uses the book's faces: Gelasio for text, DejaVu Sans Mono for addresses and code, both bundled in [`src/app/fonts`](src/app/fonts/) with their licenses, and Denim INK for headings. Denim INK is licensed to Quicknode and is not bundled; a machine with it installed shows it, and others fall back to Helvetica and Arial, the faces the book's own build names after it. Colors are the book's figure palette: ink `#111` on white, a `#f4f4f2` panel, greys for secondary text, and one green, `#1e7a3c`, kept for what moves tokens.

## Setup

Prerequisites: Node.js 20.18 or later and [pnpm](https://pnpm.io/installation).

```bash
pnpm install
cp .env.example .env.local   # then edit; see below
pnpm dev
```

### Settings (`.env.local`)

| Variable | Meaning |
| --- | --- |
| `NEXT_PUBLIC_CLUSTER` | `localnet`, `devnet` (the default), `testnet` or `mainnet`. The app offers only wallets that support this cluster. |
| `NEXT_PUBLIC_RPC_URL` | Your RPC endpoint. Defaults to the cluster's public endpoint; a [Quicknode](https://www.quicknode.com/chains/sol) endpoint is the better choice for anything beyond light testing. |
| `NEXT_PUBLIC_RPC_SUBSCRIPTIONS_URL` | The WebSocket endpoint, if it is not the RPC URL with `https` swapped for `wss`. |
| `NEXT_PUBLIC_ESCROW_PROGRAM_ADDRESS` | Where the program is deployed. Defaults to the address in `declare_id!`. |
| `NEXT_PUBLIC_SNS_RPC_URL` | A mainnet RPC endpoint for looking up `.sns` names. Defaults to the app's RPC on mainnet, and to the public mainnet endpoint otherwise. |

Next.js builds these into the app, so restart `pnpm dev` or rebuild after changing them.

### Deploying the program

The app needs the program on the cluster it talks to. If it is not there, the offers page says so.

For **devnet**, build and deploy from [`../anchor-v1`](../anchor-v1/) (or [`../anchor`](../anchor/) with the Anchor v2 CLI):

```bash
anchor build
anchor deploy --provider.cluster devnet
```

`anchor deploy` deploys to the address of the program keypair in `target/deploy/escrow-keypair.json`. If that is not the address in `declare_id!`, set `NEXT_PUBLIC_ESCROW_PROGRAM_ADDRESS` to the address it reports.

For **localnet**, load the built program into a local validator:

```bash
solana-test-validator --reset --bpf-program qbuMdeYxYJXBjU6C6qFKjZKjXmrU83eDQomHdrch826 ../anchor-v1/target/deploy/escrow.so
```

and set `NEXT_PUBLIC_CLUSTER=localnet`. A wallet appears in the app only if it lists the `solana:localnet` chain; many browser wallets list devnet but not localnet, which makes devnet the easier cluster to try the app on.

## Scripts

| Command | What it does |
| --- | --- |
| `pnpm dev` | Run the app. |
| `pnpm build` | Build for production. |
| `pnpm typecheck` | Type-check the app, scripts and tests. |
| `pnpm test` | Unit tests (see below). |
| `pnpm test:e2e` | End-to-end tests in Chromium (see below). |
| `pnpm build-idl` | Rebuild `idl/escrow.json` from the Anchor v1 program. Needs Rust; no Solana toolchain. |
| `pnpm generate-client` | Regenerate `src/generated` from `idl/escrow.json`. |
| `pnpm check-popular-tokens` | Check that every popular-token address is a mint on a cluster (see above). |

After changing the program's interface, run `pnpm build-idl` then `pnpm generate-client`, and commit both.

## Testing

**Unit tests** (`pnpm test`, in [`tests/`](tests/)) check the client against the program rather than against the IDL it came from: the instruction and account discriminators are recomputed from `sha256("global:<handler>")` and `sha256("account:Offer")`, instruction data is compared byte for byte with the Borsh layout of each handler's arguments, the offer PDA is derived from its seeds, and the program address is read out of `declare_id!`. They also cover loading offers through a fake RPC (amounts from the vault's balance, names from Token-2022 metadata, offers whose vault is missing left out), amount parsing and formatting with no floating point, prices, and the error messages a failed transaction turns into.

**End-to-end tests** (`pnpm test:e2e`, in [`tests/e2e/`](tests/e2e/)) build the app, serve it, and drive it in Chromium, using the Chromium that [Playwright](https://playwright.dev) installs (`npx playwright install chromium` if you have none). A test wallet, registered through the Wallet Standard, holds a real Ed25519 key in the page and approves every prompt. The app talks to a mock cluster: an in-memory JSON-RPC and RPC Subscriptions server that verifies each transaction's signatures and checks every escrow instruction's accounts against the constraints the program declares (the offer's PDA, each token account as the associated token account of the owner and mint the constraints name, the maker and mints the offer stored, the taker's bounds) before applying the handler's effect. The app is built for devnet, and the mock holds three of the devnet popular tokens at their real devnet addresses (SOL, USDC and PYUSD), a primary `.sns` name for the test wallet, and the System Program, associated token account and token program instructions that wrap and unwrap SOL. The tests:

- connect the wallet and see its `.sns` name;
- filter the offers, take one, make one, and cancel it;
- run the bait and switch: the maker's offer changes after the page loads, and the take is refused with nothing moved;
- see the presets that are mints on the cluster offered and the rest hidden, and the ones that can't pair greyed out;
- enter an invalid, then a valid, mint address under **Other…**;
- take an offer of SOL, take an offer wanting SOL, offer SOL and cancel, and unwrap SOL received from a take, each checked to the lamport;
- run `check-popular-tokens` against the mock.

The mock cluster checks the app against the program's interface, not the program itself. The program's behavior is tested by its own Rust tests against the compiled program; see [`../anchor/README.md`](../anchor/README.md).

## Layout

```text
idl/escrow.json          The program's IDL, from `pnpm build-idl`
scripts/                 build-idl.ts and generate-client.ts
src/generated/           The Codama client (generated; do not edit)
src/lib/                 Kit client, offer and wallet reads, instruction builders, amounts, errors
src/hooks/               React hooks over the client
src/components/          The pages' parts, each a named component with named CSS classes
src/components/token-picker/  The token chips, and popular-token-addresses.ts
src/app/                 Next.js routes, layout, fonts and globals.css
tests/                   Unit tests
tests/e2e/               End-to-end tests, mock cluster and test wallet
```
