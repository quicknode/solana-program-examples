# Anchor Hackathon Prize Program (Squads multisig committee)

A small Anchor 1.0 program for running a hackathon where a Squads multisig
committee controls prize creation and award decisions, but anyone can trigger
the actual onchain payment once a winner is recorded.

## Why it exists

Real hackathon organisers want:

1. **A committee, not a single key.** No one person can quietly mint a
   prize, change the winner, or run off with the funds.
2. **Public, auditable awards.** Once the committee has voted "Alice wins
   prize #3", anyone can execute the payout — the committee doesn't have to
   stay online to hit a button.
3. **Surplus reclaim.** If a prize is funded but never claimed, the
   committee can refund it.

This program does the onchain half. Squads handles the offchain voting and
PDA-signing flow.

## How the multisig integration works

The program is multisig-agnostic. Each `Hackathon` account stores a single
`authority: Pubkey`, and every privileged instruction handler checks
`signer == authority`. In practice that pubkey is a Squads vault PDA: the
committee proposes a vault transaction, votes on it, and when the threshold
is reached the Squads program signs the inner instruction with the vault's
PDA. Our program just sees a signed CPI from the vault and proceeds.

This means:

- You can swap Squads for any other multisig (Realms, Mean, a custom one)
  without touching this program.
- The program doesn't need to know multisig threshold, member set, or
  voting state.
- The program stays under 350 KB of compiled BPF.

## Accounts

```text
Hackathon
  authority      : Pubkey       // Squads vault PDA
  name           : String       // human-readable; hashed into seeds
  prize_count    : u8           // monotonic counter for Prize PDA seeding
  bump           : u8
  seeds = ["hackathon", authority, sha256(name)]

Prize
  hackathon : Pubkey
  index     : u8                // stable assignment from prize_count
  mint      : Pubkey            // one mint per prize
  amount    : u64               // exact payout amount
  winner    : Option<Pubkey>
  paid      : bool
  cancelled : bool
  bump      : u8
  seeds = ["prize", hackathon, index]

Vault   = ATA(prize, mint)      // Prize PDA owns its own vault
```

Per-prize mints let one hackathon mix denominations (USDC for cash
prizes, governance tokens for runner-up awards). Storing the prize index
in the PDA seed avoids reallocating the `Hackathon` account every time a
prize is added.

## Instruction handlers

| Handler            | Signer           | Behaviour                                                              |
| ------------------ | ---------------- | ---------------------------------------------------------------------- |
| `create_hackathon` | Multisig         | Initialise `Hackathon` under `authority`.                              |
| `add_prize`        | Multisig         | Register a `Prize` with its own mint and a new vault ATA.              |
| `set_winner`       | Multisig         | Record the winner pubkey for a prize.                                  |
| `pay_winner`       | **Anyone**       | Transfer exactly `prize.amount` to the winner's token account.         |
| `cancel_prize`     | Multisig         | Drain the vault to a refund target and lock the prize against payout. |
| `close_hackathon`  | Multisig         | Refund `Hackathon` rent once every prize is paid or cancelled.        |

`pay_winner` being permissionless is deliberate. Once the committee has
voted, anyone — the winner, a bot, an organiser's intern — can submit the
transaction. The committee doesn't need to stay online to deliver prizes.

## Token model

SPL Token Interface throughout (`InterfaceAccount<Mint>`,
`InterfaceAccount<TokenAccount>`, `Interface<TokenInterface>`,
`transfer_checked` from `anchor_spl::token_interface`). The same compiled
program works for both classic SPL Token and Token-2022 mints; the choice
is made per prize, at `add_prize` time, by passing the relevant mint.

## Tests

LiteSVM-based Rust integration tests build a real Squads v4 multisig
(Alice / Bob / Carol, threshold 2-of-3) and drive the program end-to-end
through Squads' propose / vote / execute flow.

The Squads onchain program is loaded from a `.so` fixture at
`programs/hackathon/tests/fixtures/squads_multisig.so`. To refresh it from
mainnet:

```
solana program dump --url mainnet-beta \
  SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf \
  programs/hackathon/tests/fixtures/squads_multisig.so
```

Squads instructions (`multisig_create_v2`, `vault_transaction_create`,
`proposal_create`, `proposal_approve`, `vault_transaction_execute`) are
built by hand in `tests/common/squads.rs`. We don't depend on the
`squads-multisig` SDK crate because it pulls in `solana-client 1.17`,
which conflicts with our Anchor 1.0 / Solana 3.x stack.

The Squads `ProgramConfig` account (normally written by a Squads admin
instruction) is forged directly into LiteSVM with
`multisig_creation_fee = 0`, so test setup is one synchronous call.

### Coverage

- **Happy path**: create → add_prize → fund → set_winner (via multisig
  vote) → pay_winner (unpermissioned). Verifies the winner's token
  balance equals `prize.amount`.
- **Failure cases**: `pay_winner` rejects when no winner is set, when the
  vault is under-funded, and when the prize has already been paid.
  `set_winner` rejects a non-multisig signer.
- **Lifecycle**: `cancel_prize` drains a funded vault to a refund target
  and locks the prize. `close_hackathon` succeeds once every prize is
  resolved and fails while any prize is still active.

## Usage

```
cargo build-sbf
cargo test
```

`cargo build-sbf` must run first because the integration tests load the
compiled `.so` via `include_bytes!`.
