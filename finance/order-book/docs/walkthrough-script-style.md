# Writing onchain program walkthrough scripts

Guidance for narrating a Solana program's full lifecycle (video scripts,
tutorials, README walkthroughs). Applies to any handler-by-handler trace.

> Holding spot for content destined for the `quicknode/solana-claude-skill`
> repo. Distilled from review feedback while editing the order-book tutorial
> script.

## 1. Show account state per step, not just prose

After each instruction, list the accounts it touched as `ADDED` / `UPDATED`
blocks. Rules that came out of review:

- **One block per account.** Never consolidate multiple accounts under a
  single `UPDATED:` heading.
- **Show the full account** (every field), with changed fields marked
  `old -> new`. Don't reduce a block to just its deltas.
- Keep field ordering identical between the `ADDED` and later `UPDATED`
  views of the same account so viewers can diff them visually.

## 2. Label every account on-curve vs off-curve — and VERIFY it

Tag each account `[off curve — PDA]` or `[on curve — keypair]`. Do **not**
infer this from intuition; read the program. Common surprises:

- "Program-owned" != PDA. Token **vaults** created with Anchor `init` +
  `token::authority = <pda>` but **no `seeds`** are *on-curve keypair*
  accounts whose *authority* is a PDA. The address is a keypair; only the
  signing authority is the PDA.
- Large **zero-copy accounts** (e.g. a ~180 KB order book) are usually
  client-generated keypairs created via `system_program::create_account`,
  **not** PDAs — Solana caps in-transaction allocations well below that size.
- **ATAs are off-curve PDAs** (derived by the Associated Token program),
  even though the wallet that owns them is on-curve.
- Genuine PDAs here: the market registry, per-user accounts, per-order
  accounts (anything with `seeds = [...]` + `bump`).

Verify against the `#[derive(Accounts)]` structs and any `create_account`
calls in the client/tests before labelling.

## 3. Add a TOKEN MOVEMENT block wherever tokens move

For every step that transfers SPL tokens, add:

```
TOKEN MOVEMENT:
    FROM:  <source account>   <amount> <token>
    TO:    <dest account>     <amount> <token>   (what this represents)
```

Annotate the *kind* of movement — collateral lock, fee sweep, settlement
payout, cancel refund — because the same vault pair appears in several
roles. Explicitly write `TOKEN MOVEMENT: none.` on steps that only update
counters (order placement that just locks, cancels, the matching engine
itself) so viewers learn that bookkeeping != transfers.

## 4. State fee accounting precisely

Pin down, with code open:

- **Where** fees are generated (typically one handler, only on a fill).
- That they're **accrued during matching but swept once per instruction**
  (a single CPI), not per fill.
- **Who pays** (taker, carved from gross) and **in which token** (usually
  quote, regardless of taker side).
- **Rounding** direction (integer division floors — show the worked number,
  e.g. `750 * 25 / 10_000 = 1.875 -> 1`).
- That resting, cancelling, and settling generate **no** fee.

## 5. Narrative conventions

- Introduce the protagonist first; keep a stable cast order across steps.
- **Bids are highest-first** (best bid on top); asks lowest-first.
- The **resting maker sets the fill price**; the taker may get price
  improvement. The taker's `Order` records the taker's *limit* price, not
  the execution price.
- A fully-filled taker order is stamped `Filled` and **never rests** on the
  book; a partial fill leaves the remainder resting and `PartiallyFilled`.

## 6. Verify all arithmetic against the source

Don't carry numbers from memory. Confirm fee bps, tick size, min order
size, and lot sizes against the program constants and tests. If the program
uses a two-lot model, state both `base_lot_size` and `quote_lot_size` and
show how they keep `price` human-readable across decimal mismatches.

## 7. Terminology

- A critbit/radix trie is **depth-bounded by key width**, not "balanced."
  Avoid implying rotations/rebalancing.
- Bits are **zero-indexed from the right**: bit *n* has value 2^n.
