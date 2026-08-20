# Vault Strategy — web app

A small, data-forward frontend for the [`vault-strategy`](../README.md) Solana program:
an educational demo of a manager-run investment vault. Investors deposit USDC and hold
shares priced at net asset value; a manager allocates the pooled USDC across a basket and
rebalances it. Every number on screen is a real on-chain account read.

Stack: **Vite + React + TypeScript + Tailwind**, `@solana/wallet-adapter`, `@coral-xyz/anchor`.
Target cluster: **devnet**.

## Setup

```bash
pnpm install
cp .env.example .env.local   # then edit — see below
pnpm dev
```

### Environment (`.env.local`)

| Variable | Meaning |
| --- | --- |
| `VITE_RPC_URL` | Your Quicknode devnet RPC endpoint. |
| `VITE_VAULT_PROGRAM_ID` | vault-strategy program id. Defaults to the repo's localnet id; **override after a devnet deploy**. |
| `VITE_ROUTER_PROGRAM_ID` | mock-swap-router program id (same note). |
| `VITE_USDC_MINT` | The USDC mint the strategy was created with (the demo mints its own). |
| `VITE_STRATEGY_INDEX` | Which strategy PDA to view (`"strategy" + index`). Default `0`. |
| `VITE_CLUSTER` | `devnet` \| `mainnet-beta` \| `custom` — controls Explorer links. |

Nothing is hardcoded: program ids come from config (defaulting to `Anchor.toml`), and the
asset list, fee, weights, NAV, and manager are all read from chain.

## Scripts

| Command | What it does |
| --- | --- |
| `pnpm dev` | Run the app. |
| `pnpm build` | Type-check and build for production. |
| `pnpm typecheck` | `tsc --noEmit`. |
| `pnpm verify` | Offline client verification (discriminators, IDL, encode/decode, PDAs). |

## Status: build layers

1. **Client + IDL + scaffold** ✅ — config-driven Anchor client, `anchor idl build`-generated
   IDL, every instruction wired to match the Rust tests, offline-verified.
2. Investor view — _next_.
3. Manager view.
4. Polish.

## Devnet reality (read this)

The programs are **not deployed to devnet**, and this environment has no Solana toolchain,
so the client is **verified offline** (`pnpm verify`) rather than against a live cluster.
To bring the demo up you need `solana` + `anchor` (or `cargo build-sbf`) to:

1. Build & deploy both programs to devnet (you'll get **new** program ids — put them in `.env.local`).
2. Create a 6-decimal mock USDC + the basket mints; init the router, set rates, fund its treasury.
3. Create a registry, approve the basket assets (bound to Pyth `PriceUpdateV2` feeds), init a
   strategy, and add the assets to 100% weight.

A one-command deploy + seed script is a planned deliverable; until then, see the Rust tests
(`programs/vault-strategy/tests/vault_strategy.rs`) for the exact sequence — the TypeScript
client mirrors it call-for-call.

## Where the IDL comes from

`src/idl/vault_strategy.json` is generated from the program source with
`anchor idl build` (Anchor 2.0.0-rc.1, the version CI installs) and committed. If you change
the program, regenerate it:

```sh
cd ../   # finance/vault-strategy/anchor
anchor idl build --program-name vault_strategy --out app/src/idl/vault_strategy.json
```

`pnpm verify` still asserts the client wiring offline (discriminators, encode/decode
round-trips, PDA derivations) as a cheap guard against a stale copy.
