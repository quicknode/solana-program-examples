# Changelog

## 2026-07-20

- **`WhitelistEntry` renamed `ApprovedAsset`** (and `whitelist_asset` renamed `approve_asset`, PDA seed `"whitelist"` renamed `"approved_asset"`), naming the account after what it is: one curator-approved asset bound to its official price feed. The unused `AssetNotWhitelisted` error is removed; approval is checked by the `ApprovedAsset` account's existence. Doc comments and README now state that the `Registry` account is the curator record at the root of the approved set, not the list itself.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
