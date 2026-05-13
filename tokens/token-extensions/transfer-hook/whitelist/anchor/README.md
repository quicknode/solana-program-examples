# Transfer Hook — Whitelist (Anchor)

A simple whitelist enforced by a Token-2022 transfer hook.

This approach doesn't scale to large whitelists: it eventually runs out of account space. A better approach for larger lists is to store entries in external PDAs (one PDA per whitelisted wallet) — see the [`pblock-list`](../../pblock-list/) example for that pattern.
