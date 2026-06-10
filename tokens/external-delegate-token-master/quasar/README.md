# External Delegate Token Master (Quasar)

Authorize token transfers using an external secp256k1 delegate signature.

See the [example overview](../README.md) for the signed-message format and nonce semantics shared with the [Anchor variant](../anchor/), and the [repository catalog](../../../README.md).

## Major concepts

- `UserAccount` state: the Solana `authority`, the delegate's 20-byte `ethereum_address`, and a `nonce` consumed by each signature-authorized transfer.
- `transfer_tokens` rebuilds the authorized message onchain as keccak256(program id || user account || amount LE || recipient token account || nonce LE), recovers the signer with the raw `sol_secp256k1_recover` syscall, compares the recovered Ethereum address to the stored one, and increments the nonce before the transfer CPI. The `authority` must also sign the transaction; the Ethereum signature supplements that check.
- `authority_transfer` moves tokens with only the Solana authority's signature.
- Both transfer handlers use the token program's `transfer_checked` CPI, which verifies the mint and decimals.
- Tokens are held by a token account owned by a PDA derived from the user account's address; the program signs the CPI with that PDA.

## Setup

From `tokens/external-delegate-token-master/quasar/`:

```bash
quasar build
```

Prerequisites: [Quasar](https://quasar-lang.com/docs) CLI and [Agave](https://docs.anza.xyz/) toolchain (see `Quasar.toml`).

## Testing

In-process tests via **Quasar SVM** (`quasar-svm` in `Quasar.toml`). Build first so `target/deploy/quasar_external_delegate_token_master.so` exists, then:

```bash
quasar test
```

The tests sign real transfer authorizations with a fixed secp256k1 key, send instructions through the SVM, and assert token balances and nonce state, including the replay, wrong-amount, wrong-recipient, and wrong-authority failure paths.

## Usage

Read `src/` and `Quasar.toml`. The [Anchor variant](../anchor/) in the same example shares the message format and state layout semantics.
