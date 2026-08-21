# Transfer Hook - Whitelist (Anchor)

> [!NOTE]
> This is the **Anchor v1** copy of this example, kept for programs staying on the
> Anchor v1 LTS line. Every `anchor` command on this page needs the v1 CLI:
> `avm install 1.1.2 && avm use 1.1.2`. The Anchor v2 version of this example is in
> [`../anchor`](../anchor/).

A whitelist enforced by a [Token Extensions](https://solana.com/docs/terminology#token-extensions-program) transfer hook. The whitelist is stored inline on a single [account](https://solana.com/docs/terminology#account).

This approach doesn't scale: the whitelist eventually runs out of account space. For larger lists, store entries in external [PDAs](https://solana.com/docs/terminology#program-derived-address-pda) (one PDA per whitelisted wallet) - see the [`block-list`](../../block-list/) example for that pattern.
