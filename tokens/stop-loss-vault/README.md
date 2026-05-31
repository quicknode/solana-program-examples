# Stop-Loss Vault

A vault that holds one volatile token for its owner and automatically sells it for a stable token once the price falls to a level the owner picked in advance. It is the onchain version of a [stop-loss order](https://www.investopedia.com/terms/s/stop-lossorder.asp) — a standing instruction to "sell if the price drops to X" so a holder caps their downside without having to watch the market.

The running example in this README uses a [tokenized stock](https://www.investopedia.com/terms/t/tokenization.asp) as the volatile token — **NVDAx** (a token that tracks the Nvidia share price) or **tslax** (Tesla) — and **USDC**, a [stablecoin](https://www.investopedia.com/terms/s/stablecoin.asp) worth ~$1, as the stable token. The program itself is asset-agnostic: any volatile-token → stable-token pair works (NVDAx → USDC, tslax → USDC, …).

## A 60-second finance primer

If you come from software rather than trading, here is everything you need:

- **Volatile token** — one whose price swings a lot (a [tokenized stock](https://www.investopedia.com/terms/t/tokenization.asp) like NVDAx or tslax). "[Volatility](https://www.investopedia.com/terms/v/volatility.asp)" just means the price moves around.
- **Stable token** — a [stablecoin](https://www.investopedia.com/terms/s/stablecoin.asp) such as USDC, engineered to stay near $1. Converting into it "locks in" a dollar value.
- **Stop-loss** — a [stop-loss order](https://www.investopedia.com/terms/s/stop-lossorder.asp): "if the price drops to my threshold, sell." This program automates exactly that.
- **Threshold price** — the floor the owner sets. At or below it, the vault converts to USDC.
- **Price oracle** — a service that reports a real-world price onchain. This vault reads a [Switchboard](https://docs.switchboard.xyz/) feed; the price travels as signed data the program can trust.
- **Swap / DEX aggregator** — software that trades one token for another at the best available price. Here that is [Jupiter](https://jup.ag); the swap consumes [liquidity](https://www.investopedia.com/terms/l/liquidity.asp) supplied by other users.
- **Cranker / keeper** — a bot that calls a program on a schedule (the chain does not run timers itself). This vault is cranked by a [TukTuk](https://github.com/helium/tuktuk) task.

Two more terms show up under [Limitations](#limitations): [slippage](https://www.investopedia.com/terms/s/slippage.asp) (getting a worse fill than quoted) and [front-running](https://www.investopedia.com/terms/f/frontrunning.asp) (someone trading ahead of your transaction).

## Major concepts

- **One vault per owner**, a [program-derived address](https://solana.com/docs/references/terminology#program-derived-account) (PDA) at seeds `[b"vault", owner.key().as_ref()]`. The PDA owns two associated token accounts — one for the volatile token, one for the stable token — and stores the oracle feed pubkey, the threshold price (in the feed's fixed-point scale), the suggested crank cadence, and the registered TukTuk task pubkey.
- **One-shot lifecycle.** A `triggered` flag flips `false → true` the moment a conversion fires. After that the pre-trigger actions (`deposit`, `update_threshold`, `withdraw_volatile`) are locked and the vault is simply a USDC wallet the owner drains with `withdraw_stables`.
- **Permissionless conversion.** `convert_if_triggered` reads the latest price and, only if it is at or below the threshold *and* fresh, performs a [cross-program invocation](https://solana.com/docs/references/terminology#cross-program-invocation-cpi) into the swap aggregator's `shared_accounts_route`, selling the vault's entire volatile balance. The vault PDA signs the swap for itself.
- **Custody.** Funds sit in program-owned token accounts. There is no admin key and no escape hatch for anyone but the owner: the owner alone can deposit, withdraw, or change the threshold, and the permissionless cranker can do nothing except execute the one swap the rules already permit. The custody rule is the deployed bytecode.

## Instructions

- `initialize_vault(threshold_price, crank_interval_seconds, tuktuk_task)` — owner creates the vault, its two associated token accounts, and records the threshold + scheduling hint.
- `deposit(amount)` — owner moves volatile tokens into the vault. Refused once triggered.
- `update_threshold(new_threshold_price?, new_crank_interval_seconds?)` — owner moves the threshold up or down (a [trailing stop](https://www.investopedia.com/terms/t/trailingstop.asp) when raised as the price rises) and/or changes the suggested cadence. Both arguments optional; refused once triggered.
- `convert_if_triggered(switchboard_price_update_data)` — permissionless. Swaps only when the latest price is at or below the threshold and fresh; otherwise reverts with `PriceAboveThreshold` (price strictly above) or `StalePrice` (price too old).
- `withdraw_stables(amount)` — owner pulls USDC out after a trigger.
- `withdraw_volatile(amount)` — owner pulls the volatile token back out *before* a trigger. The escape hatch so a vault whose threshold is never reached never locks the deposit. Refused once triggered.

## Program flow

Four users, each with a real reason to be here:

- **Alice — the vault owner (investor).** She holds 100 NVDAx because she's bullish on Nvidia long-term (her [investment thesis](https://www.investopedia.com/terms/i/investment-thesis.asp)), but she wants to cap her downside and doesn't want to watch the chart all day. Her motivation: automatic protection.
- **Bob — the cranker (keeper).** He runs a TukTuk worker that calls the vault on schedule. His motivation: keepers are paid (out of the task's funding) for reliably executing scheduled work. He cannot move funds anywhere except through the swap the rules allow.
- **Carol — an outsider.** She'd profit by draining someone else's vault. Her motivation is theft; the program's job is to refuse her.
- **Dave — a liquidity provider.** He deposits NVDAx and USDC into the swap pool the conversion routes through, acting as a [market maker](https://www.investopedia.com/terms/m/marketmaker.asp). His motivation: earn the swap fee every time a trade — including Alice's conversion — flows through his liquidity.

The lifecycle, naming the handler called and the accounts each one changes:

1. **Alice opens her vault.** Calls `initialize_vault($200 threshold, 600s cadence, tuktuk_task)`.
   *Creates:* the `vault` PDA, the vault's NVDAx associated token account, the vault's USDC associated token account.
2. **Alice funds it.** Calls `deposit(100 NVDAx)`.
   *Changes:* Alice's NVDAx account (−100), the vault's NVDAx account (+100). The move uses `transfer_checked`, which carries the mint and decimals so a wrong-token account can't slip through.
3. **Bob cranks on schedule.** Calls `convert_if_triggered(price_update)` every 10 minutes.
   - While NVDAx is above $200: reverts with `PriceAboveThreshold` — a cheap no-op that changes nothing.
   - When NVDAx is at or below $200 and the price is fresh: swaps all 100 NVDAx for USDC through Dave's pool via Jupiter's `shared_accounts_route`. *Changes:* the vault's NVDAx account (→ 0), the vault's USDC account (+ 100 × price), Dave's pool accounts, and `vault.triggered` (→ `true`).
4. **Alice trails her floor up (optional).** NVDAx rallies to $300, so she raises her protection. Calls `update_threshold($250)`.
   *Changes:* `vault.threshold_price`. (Rejected if the vault has already triggered.)
5. **Alice cashes out after a conversion.** Calls `withdraw_stables(amount)`.
   *Changes:* the vault's USDC account (−amount), Alice's USDC account (+amount). Callable only after a trigger.
6. **…or Alice backs out if it never fires.** NVDAx stays above her floor and she changes her mind. Calls `withdraw_volatile(amount)` before any trigger.
   *Changes:* the vault's NVDAx account (−amount), Alice's NVDAx account (+amount). Refused after a trigger (the balance is in USDC by then — she'd use `withdraw_stables`).
7. **Carol tries to steal.** Calls `withdraw_stables` or `withdraw_volatile` pointed at Alice's vault. The `has_one = owner` constraint and the PDA seeds reject her: the transaction reverts and **no accounts change**.

| Handler | Who calls it | Accounts it changes |
| --- | --- | --- |
| `initialize_vault` | Owner (Alice) | Creates `vault` PDA + vault's two token accounts |
| `deposit` | Owner | Owner's volatile account → vault's volatile account |
| `update_threshold` | Owner | `vault.threshold_price` / `vault.crank_interval_seconds` |
| `convert_if_triggered` | Anyone (cranker Bob) | Vault's volatile account → vault's stable account (via swap pool); sets `vault.triggered` |
| `withdraw_stables` | Owner | Vault's stable account → owner's stable account |
| `withdraw_volatile` | Owner | Vault's volatile account → owner's volatile account |

## Why Switchboard On-Demand

Switchboard On-Demand prices are pulled (not pushed) and verified onchain via Ed25519 signatures, so the price-update bytes travel as an instruction argument and the program trusts them only after verification. That fits a permissionless crank: the cranker pays for the price update it wants the program to act on, and the program never trusts the cranker's identity. Pyth is the obvious alternative but pushes prices on a continuous publisher schedule, which costs more in account rent and update fees for the same behaviour.

The teaching example uses a `mock-switchboard` program exposing the minimum fields the vault reads (price, scale, last-update slot) so the tests can drive deterministic price scenarios. Production swaps it for the `switchboard-on-demand` crate and verifies updates via `PullFeedAccountData::parse_and_verify`.

## Why TukTuk

[TukTuk](https://github.com/helium/tuktuk) is the maintained replacement for Clockwork (dead) for scheduling onchain instruction handlers. The vault doesn't enforce the cadence onchain — it records `crank_interval_seconds` as a hint and stores the TukTuk task pubkey for discoverability. Anyone can crank, but in normal operation TukTuk runs the schedule and pays for the price update.

## Setup

Prerequisites: the [Solana CLI](https://docs.anza.xyz/cli/install), [Anchor](https://www.anchor-lang.com/docs/installation) 1.0+, and a Rust toolchain. From `tokens/stop-loss-vault/anchor`, `anchor build` compiles all three programs (the vault plus the `mock-jupiter` and `mock-switchboard` test stand-ins) to `target/deploy/`, which the integration tests load via `include_bytes!`.

## Testing

```sh
anchor build
anchor test
```

`anchor test` runs the Rust + LiteSVM integration tests under `programs/stop-loss-vault/tests/stop_loss_vault_scenarios.rs`. The fixtures use a 9-decimal NVDAx stand-in priced in USD against 6-decimal USDC:

- Alice opens a vault with a $100 threshold and deposits 10 NVDAx.
- Bob cranks across three checks ($180 → $150 → $80); the third fires the conversion and Alice withdraws $800 USDC (10 × $80).
- Carol cannot withdraw from a vault she doesn't own.
- Alice trails the threshold up to $200 after NVDAx rallies to $250; the next crank fires at $180 (10 × $180 = $1800 USDC).
- A crank when the price is above the threshold reverts cheaply and leaves the vault un-triggered.
- A [flash crash](https://www.investopedia.com/terms/f/flash-crash.asp) *between* cranks is missed — the vault is not converted (see Limitations).
- A price that drops below the threshold but goes stale before the next crank is rejected — the vault is not converted.
- A vault that never triggers isn't a trap: Alice pulls her deposit back out with `withdraw_volatile`, and Carol can't use it against Alice's vault.

## Limitations

- **Flash-crash gap between cranks.** This is a discrete-time stop-loss: the vault only sees the price at crank time. If the price crashes through the threshold and recovers between two consecutive cranks, the vault never sees the crash and the conversion does not fire. The fix is either a tighter `crank_interval_seconds` (more crank and price-update fees) or a continuous-watch offchain liquidator with stronger trust assumptions. `test_flash_crash_between_cranks_misses_trigger` demonstrates the gap.
- **Oracle staleness.** `convert_if_triggered` rejects a price whose `last_update_slot` is more than `MAX_PRICE_STALENESS_SLOTS` behind the current slot, so a crank can't act on a stale print from a feed that has stopped updating. Switchboard On-Demand prices are pulled at crank time, so in normal operation the price is always fresh; the check exists to fail closed when it isn't.
- **MEV / [front-running](https://www.investopedia.com/terms/f/frontrunning.asp).** `convert_if_triggered` is permissionless and swaps at whatever route the cranker supplies, so the swap is exposed to adversarial transaction ordering — a searcher, or the slot leader building the block, can place a transaction around the crank to fill it at a worse price. The Jupiter route built here passes `slippage_bps = 0` and `quoted_out_amount = 0` for simplicity; production must compute a real quote and pass realistic [slippage](https://www.investopedia.com/terms/s/slippage.asp), or route privately (for example through a Jito bundle), to avoid being filled below the oracle's last print.
- **No partial-fill protection.** The vault swaps its *entire* volatile balance in one instruction. If [liquidity](https://www.investopedia.com/terms/l/liquidity.asp) for the full size is thin, the owner pays the route's price impact in full. Real systems split into chunks or refuse to convert above a price-impact ceiling.
- **`mock-jupiter` is a test stand-in.** It performs a deterministic price-multiply rather than a real route. Do not deploy with it. To use real Jupiter v6, pass its program ID as the `swap_program` account at call time — the handler derives the instruction discriminator from the `shared_accounts_route` name, so it already matches the real program.
- **`mock-switchboard` is a test stand-in.** It exposes a writable price the test harness drives directly. Real Switchboard On-Demand verifies signed updates onchain via `PullFeedAccountData::parse_and_verify`; the production handler must do the same and reject unsigned data.
- **TukTuk task registration is stubbed.** `initialize_vault` accepts a `tuktuk_task` pubkey as input rather than CPI-creating the task atomically. See the `TODO` in `initialize_vault.rs` for the integration point.
