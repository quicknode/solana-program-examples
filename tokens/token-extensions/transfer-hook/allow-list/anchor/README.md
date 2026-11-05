# Transfer Hook - Allow List (Anchor)

> [!NOTE]
> This is the **Anchor v2** copy of this example. Every `anchor` command on this page
> needs the v2 CLI: `cargo install anchor-cli --version 2.0.0-rc.2 --locked` (avm has
> no prebuilt binary for this pre-release). The Anchor v1 version of this example is in
> [`../anchor-v1`](../anchor-v1/).

A [Token Extensions](https://solana.com/docs/terminology#token-extensions-program) transfer hook that only lets a mint's tokens reach wallets on that mint's allow list. Every transfer of the mint calls the hook, and the hook rejects the transfer unless the recipient wallet is allowed.

See also: the [repository catalog](../../../../../README.md), and [Allow/Block List](../../allow-block-list-token/) for a list that can also block wallets or allow transfers below a threshold.

## Major concepts

- Transfer hook with one extra account, resolved per transfer
- One account per allowed wallet, so the list has no size limit
- Only the mint authority can set up a mint's allow list

## Accounts

- The **allow list** account lives at a PDA with seeds `"allow_list"` and the mint. It holds the `authority` that may allow and disallow wallets: the mint authority who created it. One per mint.
- An **allowed wallet** account lives at a PDA with seeds `"allowed_wallet"`, the mint and the wallet. Its existence is the permission. `allow_wallet` creates it and `disallow_wallet` closes it, returning the rent to the authority.

Because each allowed wallet is its own account, allowing the ten-thousandth wallet costs the same as allowing the first.

## How a transfer is checked

`initialize_extra_account_meta_list` stores one extra account in the mint's `ExtraAccountMetaList`: the allowed wallet PDA, with seeds `"allowed_wallet"`, the mint (account 1 of the transfer) and the destination token account's owner (account 2, bytes 32 to 64). On every transfer Token Extensions derives that address for the actual recipient and passes the account to the hook. The hook re-derives the address from the destination token account and fails with `RecipientNotAllowed` unless an allowed wallet account exists there.

The hook checks the wallet that owns the destination token account, not the token account itself, so allowing a wallet covers every token account it holds for that mint.

## Instruction handlers

- `initialize_extra_account_meta_list`: creates the mint's `ExtraAccountMetaList` and allow list. The signer must be the mint authority, and becomes the allow list's authority.
- `allow_wallet`: the allow list's authority adds a wallet.
- `disallow_wallet`: the allow list's authority removes a wallet.
- `transfer_hook`: called by Token Extensions on every transfer.

## Setup

From this directory (`tokens/token-extensions/transfer-hook/allow-list/anchor/`):

```bash
anchor build
```

Prerequisites: [Agave](https://docs.anza.xyz/) CLI (version in `Anchor.toml` `[toolchain]`), [Anchor](https://www.anchor-lang.com/docs).

## Testing

Tests run in-process with [LiteSVM](https://www.anchor-lang.com/docs/testing/litesvm). No local validator.

```bash
anchor test
```

This runs `cargo test` as configured in `Anchor.toml`. The tests make real Token Extensions transfers through the hook. They check that a transfer reaches an allowed wallet; that a wallet not on the list, a disallowed wallet, or a wallet allowed only for another mint is refused; that fifty wallets can be allowed; that only the mint authority can create the allow list; and that only its authority can allow a wallet.

## Usage

Read the program `programs/` source and `Anchor.toml` for deployed program IDs. For deployment, use `anchor build && anchor deploy` against your target cluster.
