# Transfer Hook - Allow List (Quasar)

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

`initialize_extra_account_meta_list` stores one extra account in the mint's `ExtraAccountMetaList`: the allowed wallet PDA, with seeds `"allowed_wallet"`, the mint (account 1 of the transfer) and the destination token account's owner (account 2, bytes 32 to 64). On every transfer Token Extensions derives that address for the actual recipient and passes the account to the hook. The hook re-derives the address from the destination token account and fails with custom error 6000 (`ERROR_RECIPIENT_NOT_ALLOWED`) unless an allowed wallet account exists there.

The hook checks the wallet that owns the destination token account, not the token account itself, so allowing a wallet covers every token account it holds for that mint.

## Instruction handlers

- `initialize_extra_account_meta_list`: creates the mint's `ExtraAccountMetaList` and allow list. The signer must be the mint authority, and becomes the allow list's authority.
- `allow_wallet`: the allow list's authority adds a wallet.
- `disallow_wallet`: the allow list's authority removes a wallet.
- `transfer_hook`: called by Token Extensions on every transfer.

## Setup

From `tokens/token-extensions/transfer-hook/allow-list/quasar/`:

```bash
quasar build
```

Prerequisites: [Quasar](https://quasar-lang.com/docs) CLI and [Agave](https://docs.anza.xyz/) toolchain (see `Quasar.toml`).

## Testing

In-process tests via **Quasar SVM** (`quasar-svm` in `Quasar.toml`):

```bash
cargo test
```

The harness runs each instruction directly, so the tests call `transfer_hook` with the accounts Token Extensions would pass during a transfer, rather than making a transfer. They check that the hook accepts an allowed wallet; that it refuses a wallet not on the list, a disallowed wallet, a wallet allowed only for another mint, and an allowed wallet account passed for a different recipient; that fifty wallets can be allowed; that only the mint authority can create the allow list; and that only its authority can allow or disallow a wallet. No local validator.

## Usage

Read `src/` and `Quasar.toml`. Compare with the [Anchor](../anchor/) variant in the same example.
