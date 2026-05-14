# Transfer Hook — Whitelist (Anchor)

A whitelist enforced by a Token Extensions transfer hook. The whitelist is stored inline on a single account.

This approach doesn't scale: the whitelist eventually runs out of account space. For larger lists, store entries in external PDAs (one PDA per whitelisted wallet) — see the [`block-list`](../../block-list/) example for that pattern.
