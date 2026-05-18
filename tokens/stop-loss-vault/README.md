# Stop-Loss Vault

A per-owner vault that holds a single volatile SPL token (e.g. wSOL) and permissionlessly converts it to a single stable SPL token (e.g. USDC) when a Switchboard On-Demand price feed reports a price at or below an owner-set threshold. The conversion is triggered by an offchain cranker — typically a [TukTuk](https://github.com/helium/tuktuk) task — that calls `convert_if_triggered` on a schedule. The instruction reverts cheaply when the price is still above the threshold and only swaps when the price has actually dropped.

## Architecture

One PDA per owner at seeds `[b"vault", owner.key().as_ref()]`. The vault owns two associated token accounts — one for the volatile mint, one for the stable mint — and records the oracle feed pubkey, the threshold price (in the feed's native fixed-point scale), the suggested crank cadence, and the registered TukTuk task pubkey. A `triggered` flag flips from `false` to `true` once a conversion has fired, locking the vault out of further deposits or threshold updates so the post-trigger state is just a stable-token wallet.

The conversion path reads the latest price from the Switchboard feed, compares it to the stored threshold, and if (and only if) the price is strictly below the threshold, CPIs the swap aggregator's `shared_accounts_route` instruction with the vault's entire volatile balance. The vault PDA signs the CPI for itself. In production the swap aggregator is Jupiter v6; in tests a `mock-jupiter` program with the same external instruction shape stands in.

## Instructions

- `initialize_vault(threshold_price, crank_interval_seconds, tuktuk_task)` — owner creates the vault, its two ATAs, and records the threshold + scheduling hint.
- `deposit(amount)` — owner moves volatile tokens into the vault. Refuses once the vault has triggered.
- `update_threshold(new_threshold_price?, new_crank_interval_seconds?)` — owner trails the threshold up (or down) and/or changes the suggested crank cadence. Both arguments optional; refuses once the vault has triggered.
- `convert_if_triggered(switchboard_price_update_data)` — permissionless. Anyone can call; the instruction only swaps when the latest price is strictly below the threshold. Otherwise it reverts with `PriceAboveThreshold`.
- `withdraw_stables(amount)` — owner pulls stables out after the vault has triggered.

## Why Switchboard On-Demand

Switchboard On-Demand prices are pulled (not pushed) and verified onchain via Ed25519 signatures, so the price-update bytes travel as an instruction argument and the program trusts them only after signature verification. That fits a permissionless crank model: the cranker pays for the price update they want the program to act on, and the program never has to trust the cranker's identity. Pyth is the obvious alternative but pushes prices on a continuous publisher schedule, which costs more in account rent and update fees for the same end behaviour.

The teaching example uses a `mock-switchboard` program with the minimum fields the vault needs (price, scale, last-update slot) so the tests can drive deterministic price scenarios. Production swaps `mock-switchboard` for the real `switchboard-on-demand` crate and verifies updates via `PullFeedAccountData::parse_and_verify`.

## Why TukTuk

[TukTuk](https://github.com/helium/tuktuk) is the maintained replacement for Clockwork (which is dead) for scheduling onchain instructions. The vault doesn't enforce the crank cadence onchain — it just records `crank_interval_seconds` as a hint and stores the TukTuk task pubkey for discoverability. Anyone can crank, but in normal operation TukTuk runs the schedule and pays for the price update.

## Testing

```sh
anchor build
anchor test
```

`anchor test` runs the Rust + LiteSVM integration tests under `programs/stop-loss-vault/tests/stop_loss_vault_scenarios.rs`. Scenarios:

- Alice initialises a vault with a $100 threshold, deposits 10 SOL.
- Bob cranks across three checks ($180 → $150 → $80); the third fires the conversion and Alice withdraws $800 USDC.
- Carol cannot withdraw from a vault she doesn't own.
- Alice trails the threshold up to $200 after SOL rallies to $250; the next crank fires at $180.
- A crank when the price is above threshold reverts cheaply and leaves the vault un-triggered.
- A flash crash *between* cranks is missed — the vault is not converted (see Limitations).

## Limitations

- **Flash-crash gap between cranks.** This is a discrete-time stop-loss. The vault only sees the price at crank time. If the price crashes through the threshold and recovers between two consecutive cranks, the vault never sees the crash and the conversion does not fire. The fix is either a tighter `crank_interval_seconds` (which costs more in crank fees and price-update fees) or a continuous-watch offchain liquidator with stronger trust assumptions. `test_flash_crash_between_cranks_misses_trigger` demonstrates the gap explicitly.
- **Oracle staleness.** The vault accepts whatever the feed currently reports. It does not enforce a maximum age on the price update. Production should reject updates older than some `max_staleness_seconds` once it's reading a real Switchboard feed.
- **MEV behaviour.** `convert_if_triggered` is permissionless, so a sandwich attacker watching the mempool can front-run the crank with adverse routes. The Jupiter route built here passes `slippage_bps = 0` and `quoted_out_amount = 0` for simplicity — production must compute a real quote and pass realistic slippage, or use a private route, to avoid being filled at a worse price than the oracle's last print.
- **No partial-fill protection.** The vault swaps its *entire* volatile balance in one instruction. If liquidity for the full size is poor, the user pays the route's price impact in full. Real systems split into chunks or refuse to convert above a price-impact ceiling.
- **`mock-jupiter` is a test stand-in.** It performs a deterministic price-multiply rather than a real route. Do not deploy with it. Swap to Jupiter v6 by changing the `swap_program` account passed at call time and pointing `instruction_data`'s discriminator at Jupiter v6's real `shared_accounts_route` sighash.
- **`mock-switchboard` is a test stand-in.** It exposes a writable price the test harness drives directly. Real Switchboard On-Demand verifies signed updates onchain via `PullFeedAccountData::parse_and_verify`; the production handler must do the same and reject unsigned data.
- **TukTuk task registration is stubbed.** `initialize_vault` accepts a `tuktuk_task` pubkey as an input rather than CPI-creating the task atomically. See the `TODO` in `initialize_vault.rs` for the integration point.
