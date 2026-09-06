# Solana Options (Quasar)

A [Quasar](https://quasar-lang.com/docs) port of the Solana options example.
The design, math, and behavior match the Anchor implementation at
[`../anchor`](../anchor). Read that README for the full walkthrough of the
fully collateralized, physically settled venue. This page only covers what
differs in the Quasar version.

## Differences from the Anchor version

- **`kind` and `status` are `u8`.** Quasar instruction arguments and
  zero-copy fields are plain integers, so the Anchor sibling's `OptionKind`
  and `OptionStatus` enums become the constants in `constants.rs`:
  `KIND_CALL` / `KIND_PUT` and `STATUS_LISTED` / `STATUS_HELD` /
  `STATUS_EXERCISED`.
- **`write_option` takes its terms as separate arguments** (`kind`,
  `contracts`, `underlying_per_contract`, `strike_per_contract`, `premium`,
  `expiry`) rather than the Anchor sibling's `OptionTerms` struct.
- **Every party's token accounts must already exist.** The Anchor version
  uses `init_if_needed` to create a call holder's underlying account and a
  put writer's underlying account at the moment they are first paid in that
  token; here the tests create both token accounts for every character up
  front.
- **The writer's premium account is bound in the handler.** The Anchor
  version derives it as the writer's associated token account; here
  `buy_option` checks that the account passed as `writer_quote` is owned by
  the writer and holds the quote token, so a buyer cannot route the premium
  to themselves (`buy_refuses_a_premium_account_the_writer_does_not_own`).
- **State writes** use Quasar's zero-copy field accessors (`field.get()` /
  `field.set()`) and `set_inner`, rather than Anchor's account mutation.

## Testing

```bash
quasar build
cargo test
```

Tests run in-process with [`quasar-test`](https://github.com/blueshift-gg/quasar).
They set up both mints, a venue at a 1% fee, and the five characters with
their token accounts, warp the clock to a fixed start time so the week-long
expiry is deterministic, then walk the call from write to collected strike
and the put from write to exercise and to expiry, checking the custody
ledger against the vault balances after every step. Every gate has a test
that proves it shuts: the expiry boundary from both sides, cancel after
sale, buy after sale or expiry, exercise by a non-holder, collection by a
non-writer or before exercise, reclaim after exercise, fee collection by a
non-admin, and the parameter checks at write time.
