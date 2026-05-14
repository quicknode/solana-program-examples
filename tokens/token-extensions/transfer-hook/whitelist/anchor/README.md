# Transfer Hook — Whitelist (Anchor)

A whitelist enforced by a [Token Extensions](https://solana.com/docs/terminology#token-extensions-program) transfer hook. The whitelist is stored inline on a single [account](https://solana.com/docs/terminology#account).

This approach doesn't scale: the whitelist eventually runs out of account space. For larger lists, store entries in external [PDAs](https://solana.com/docs/terminology#program-derived-address-pda) (one PDA per whitelisted wallet) — see the [`block-list`](../../block-list/) example for that pattern.
