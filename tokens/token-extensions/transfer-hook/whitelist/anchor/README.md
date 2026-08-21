# Transfer Hook - Whitelist (Anchor)

> [!NOTE]
> This is the **Anchor v2** copy of this example. Every `anchor` command on this page
> needs the v2 CLI: `cargo install anchor-cli --version 2.0.0-rc.1 --locked` (avm has
> no prebuilt binary for this pre-release). The Anchor v1 version of this example is in
> [`../anchor-v1`](../anchor-v1/).

A whitelist enforced by a [Token Extensions](https://solana.com/docs/terminology#token-extensions-program) transfer hook. The whitelist is stored inline on a single [account](https://solana.com/docs/terminology#account).

This approach doesn't scale: the whitelist eventually runs out of account space. For larger lists, store entries in external [PDAs](https://solana.com/docs/terminology#program-derived-address-pda) (one PDA per whitelisted wallet) - see the [`block-list`](../../block-list/) example for that pattern.
