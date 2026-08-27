# Hackathon Prize Program (Squads multisig committee)

An [Anchor](https://www.anchor-lang.com/docs) program for running a hackathon
where a *committee* — not a single person — controls the prize money. The
committee decides who wins; once a winner is recorded, anyone can trigger the
actual payout.

New to Solana or to finance terms? Start with [Concepts](#concepts) — every
term below is linked to a plain-language definition the first time it appears.

## What problem this solves

Organisers handling real prize money want three things:

1. **A committee, not a single key.** No one person can create a prize, change
   a winner, or walk off with the funds.
2. **Public, auditable payouts.** Once the committee has agreed "Dave wins the
   AI prize", *anyone* can submit the payout transaction — the committee does
   not have to stay online to release the money.
3. **A way to reclaim unspent prizes.** If a track gets no valid entries, the
   committee can refund the money it set aside.

This program is the onchain half. [Squads](https://docs.squads.so), a multisig
(explained below), handles the offchain proposing and voting.

## Concepts

If you already write Solana programs, skip ahead to the
[walkthrough](#walkthrough-alice-bob-and-carol-run-solana-bash). Otherwise, here
is everything this program touches, in plain terms:

- **[Program](https://solana.com/docs/references/terminology)** — code deployed
  onchain (Solana's name for what other chains call a "smart contract"). It owns
  no money of its own; it moves tokens only when its rules allow.
- **Onchain / offchain** — "onchain" means stored on and enforced by the Solana
  network; "offchain" means everything else (a laptop, a server, a vote held in
  a Squads app).
- **[Account](https://solana.com/docs/references/terminology)** — every piece of
  onchain data lives in an account. A
  **[program derived address (PDA)](https://solana.com/docs/references/terminology)**
  is an account whose address is derived from a program plus some seeds, so the
  program itself can sign for it. This program keeps two kinds of PDA:
  `Hackathon` and `Prize`.
- **[Token](https://solana.com/docs/references/terminology) and mint** — a token
  is a transferable asset; its *mint* is the account that defines it (its supply
  and its number of decimal places). One mint per kind of prize.
- **[Associated token account (ATA)](https://solana.com/docs/references/terminology)**
  — the standard account that holds a balance of one token for one owner. Each
  prize gets its own ATA, owned by the prize's PDA — this is the prize's
  **vault**.
- **[Escrow](https://www.investopedia.com/terms/e/escrow.asp)** — money held by a
  neutral third party until conditions are met. Here the "neutral third party"
  is the program: prize tokens sit in the vault and can only leave by the rules
  below (pay the recorded winner, or refund on cancel).
- **[USDC](https://solana.com/docs/references/terminology)** — a
  [stablecoin](https://www.investopedia.com/terms/s/stablecoin.asp): a token
  whose value is pegged to one US dollar. Good for cash prizes.
- **Tokenized stocks (`NVDAx`, `TSLAx`)** — tokens that track the price of a
  real-world [stock](https://www.investopedia.com/terms/s/stock.asp) such as
  NVIDIA (`NVDAx`) or Tesla (`TSLAx`). They behave like any other token onchain,
  so a prize can be denominated in them.
- **Multisig** — a "multi-signature" wallet that needs several people to approve
  before it can act (for example 2 of 3). [Squads](https://docs.squads.so) is
  the multisig used here; it is not part of this program (see
  [Trust model](#trust-model)).
- **[Rent](https://solana.com/docs/references/terminology)** — the small SOL
  deposit an account holds to stay alive onchain. Closing an account returns its
  rent.
- **`transfer_checked` and decimals** — every payout uses Solana's
  `transfer_checked`, which carries the mint's decimal count through the
  transfer so the amount can never be misread. Amounts are always stored in base
  units (USDC has 6 decimals, so `10000 USDC` is stored as `10_000_000000`).

## Walkthrough: Alice, Bob, and Carol run "Solana Bash"

**The cast.** Alice, Bob, and Carol are the organising committee. They set up a
2-of-3 [Squads](https://docs.squads.so) multisig — any two of them must approve
before the committee acts. The multisig's *vault PDA* becomes the hackathon's
`authority`.

**The money and the motivation.** A fund that is bullish on AI sponsors the
event because it wants to grow the pool of builders shipping AI tools on Solana
— its thesis is that a bigger ecosystem makes its own holdings more valuable. It
puts up three prizes in three different tokens:

| Prize | Track          | Token   | Amount |
| ----- | -------------- | ------- | ------ |
| 0     | Grand prize    | `USDC`  | 10,000 |
| 1     | Best AI app    | `NVDAx` | 5      |
| 2     | Best fintech   | `TSLAx` | 3      |

Here is the full lifecycle. Each step names the instruction handler called and
the accounts it creates or changes.

1. **Create the hackathon — `create_hackathon`.** Signed by the committee (a
   Squads vote). Creates the `Hackathon` PDA, storing `authority` (the vault
   PDA), the name, and `prize_count = 0`. A human payer covers the rent so the
   vault PDA does not have to.

2. **Add the three prizes — `add_prize` (called three times).** Signed by the
   committee. Each call creates a `Prize` PDA (recording its `index`, `mint`,
   and `amount`, with `winner = None`, `paid = false`, `cancelled = false`) and
   the prize's **vault** (an ATA owned by that `Prize` PDA). It also bumps
   `Hackathon.prize_count`. The amount must be greater than zero.

3. **Fund the vaults.** The committee sends 10,000 `USDC` to prize 0's vault, 5
   `NVDAx` to prize 1's vault, and 3 `TSLAx` to prize 2's vault. This is an
   ordinary token transfer into the vault ATAs, not a call to this program — the
   prizes are now held in escrow.

4. **Record a winner — `set_winner`.** Signed by the committee. Dave wins the AI
   track, so the committee records him on prize 1: `Prize.winner = Some(Dave)`.
   No tokens move yet.

5. **Pay the winner — `pay_winner`.** **Permissionless** — anyone can call it.
   Dave (or a bot, or an organiser's intern) submits it. The handler checks the
   winner is set, the vault holds enough, and the prize is not already paid or
   cancelled, then uses `transfer_checked` to send exactly `prize.amount` (5
   `NVDAx`) from the vault to Dave's token account and sets `Prize.paid = true`.
   The committee never had to come back online.

6. **Cancel an unawarded prize — `cancel_prize`.** The fintech track got no
   valid entries. Signed by the committee, this drains prize 2's vault back to a
   refund token account, closes the vault (returning its rent), and sets
   `Prize.cancelled = true` so it can never be paid.

7. **Close the hackathon — `close_hackathon`.** Signed by the committee. Once
   every prize is either paid or cancelled, this closes the `Hackathon` account
   and returns its rent. The individual `Prize` accounts are left in place as a
   permanent record of who won what.

## Accounts

```text
Hackathon
  authority      : Pubkey       // the committee's multisig vault PDA
  name           : String       // human-readable; hashed into the seeds
  prize_count    : u8           // how many prizes exist; seeds the next Prize
  bump           : u8
  seeds = ["hackathon", authority, sha256(name)]

Prize
  hackathon : Pubkey
  index     : u8                // fixed at creation; never changes
  mint      : Pubkey            // the token this prize is paid in
  amount    : u64               // exact payout, in base units
  winner    : Option<Pubkey>    // None until set_winner runs
  paid      : bool
  cancelled : bool
  bump      : u8
  seeds = ["prize", hackathon, index]

Vault = ATA(prize, mint)        // each Prize PDA owns its own vault
```

One mint per prize lets a single hackathon mix tokens — `USDC` for cash, a
tokenized stock for a themed track. Storing the prize index in the PDA seed
means adding a prize never has to resize the `Hackathon` account.

## Instruction handlers

| Handler            | Signer         | What it does                                         | Accounts created / changed                                                  |
| ------------------ | -------------- | ---------------------------------------------------- | --------------------------------------------------------------------------- |
| `create_hackathon` | Committee      | Start a hackathon under the committee's authority.   | Creates `Hackathon`.                                                        |
| `add_prize`        | Committee      | Register a prize (nonzero amount) and its vault.     | Creates `Prize` + its vault ATA; increments `Hackathon.prize_count`.        |
| `set_winner`       | Committee      | Record the winner for a prize.                       | Sets `Prize.winner`.                                                        |
| `pay_winner`       | **Anyone**     | Pay exactly `prize.amount` to the recorded winner.   | Moves tokens vault → winner; sets `Prize.paid`.                             |
| `cancel_prize`     | Committee      | Refund a prize's vault and lock it against payout.   | Moves tokens vault → refund target; closes vault; sets `Prize.cancelled`.   |
| `close_hackathon`  | Committee      | Wind down once every prize is resolved.              | Closes `Hackathon` (returns rent). `Prize` accounts are kept.               |

`pay_winner` being permissionless is deliberate: once the committee has agreed
on a winner, anyone can deliver the payout, so the committee does not need to
stay online.

## Trust model

The prize tokens are held in program-owned vault PDAs — an escrow. No single key
can move them; payouts follow only the rules in the deployed program:
`pay_winner` can send only the recorded `amount` to the recorded `winner`, and
only the committee's multisig can `set_winner` or `cancel_prize`. The committee
is two of Alice, Bob, and Carol acting together, never one of them alone.

The program is multisig-agnostic: it stores one `authority` pubkey and checks
`signer == authority` on every committee-only handler. In practice that pubkey
is a Squads vault PDA — the committee proposes, votes, and on reaching the
threshold the Squads program signs the inner instruction with the vault via a
[cross-program invocation](https://solana.com/docs/references/terminology). You
could swap Squads for another multisig without changing a line of this program.

## Token model

The token interface is used throughout (`InterfaceAccount<Mint>`,
`InterfaceAccount<TokenAccount>`, `Interface<TokenInterface>`,
`transfer_checked` from `anchor_spl::token_interface`). The same compiled
program works for both the Classic Token Program and the Token Extensions
Program; the choice is made per prize, at `add_prize` time, by passing the
relevant mint.

## Tests

[LiteSVM](https://www.anchor-lang.com/docs/testing/litesvm)-based Rust
integration tests build a real Squads v4 multisig (Alice / Bob / Carol,
threshold 2-of-3) and drive the program end-to-end through Squads'
propose / vote / execute flow.

The Squads onchain program is loaded from a `.so` fixture at
`programs/hackathon/tests/fixtures/squads_multisig.so`. To refresh it from
mainnet:

```sh
solana program dump --url mainnet-beta \
  SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf \
  programs/hackathon/tests/fixtures/squads_multisig.so
```

Squads instructions (`multisig_create_v2`, `vault_transaction_create`,
`proposal_create`, `proposal_approve`, `vault_transaction_execute`) are built by
hand in `tests/common/squads.rs` rather than via the `squads-multisig` SDK
crate, which pulls in `solana-client 1.17` and conflicts with the Anchor 1.0 /
Solana 3.x stack. The Squads `ProgramConfig` account is written directly into
LiteSVM with `multisig_creation_fee = 0`, so test setup is one synchronous call.

### Coverage

- **Happy path**: create → add_prize → fund → set_winner (via multisig vote) →
  pay_winner (permissionless). Verifies the winner's token balance equals
  `prize.amount`.
- **Failure cases**: `add_prize` rejects a zero amount; `pay_winner` rejects when
  no winner is set, when the vault is under-funded, and when the prize has
  already been paid; `set_winner` rejects a non-committee signer.
- **Lifecycle**: `cancel_prize` drains a funded vault to a refund target and
  locks the prize; `close_hackathon` succeeds once every prize is resolved and
  fails while any prize is still active.

## Usage

```sh
cargo build-sbf
cargo test
```

`cargo build-sbf` must run first because the integration tests load the compiled
`.so` via `include_bytes!`.
