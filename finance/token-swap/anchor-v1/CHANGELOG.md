# Changelog

## 2026-09-10

Removed the separate dataless signer PDA (seeds `[config, mint_a, mint_b,
b"authority"]`) that owned the reserves and the LP mint. The `PoolConfig`
account now owns both reserves (`pool_a` and `pool_b` are its associated token
accounts), is the LP mint's mint authority, and signs the transfers out of the
reserves and the LP mint/burn CPIs with its own seeds
`[config, mint_a, mint_b, bump]`, the way the escrow example's `offer` account
signs for its vault. The `b"authority"` seed constant and the extra signer
account are gone from every instruction, and the reserve addresses changed with
their owner: derive them as the associated token accounts of `pool_config`.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
