# External Delegate Token Master (Anchor)

> [!NOTE]
> This is the **Anchor v1** copy of this example, kept for programs staying on the
> Anchor v1 LTS line. Every `anchor` command on this page needs the v1 CLI:
> `avm install 1.1.2 && avm use 1.1.2`. The Anchor v2 version of this example is in
> [`../anchor`](../anchor/).

Authorize token transfers using an external secp256k1 delegate signature.

See the [example overview](../README.md) for the signed-message format and nonce semantics shared with the [Quasar variant](../quasar/), and the [repository catalog](../../../README.md).

## Major concepts

- `UserAccount` state: the Solana `authority`, the delegate's 20-byte `ethereum_address`, and a `nonce` consumed by each signature-authorized transfer.
- `transfer_tokens` rebuilds the authorized message onchain as keccak256(program id || user account || amount LE || recipient token account || nonce LE), recovers the signer with the secp256k1 syscall, compares the recovered Ethereum address to the stored one, and increments the nonce before the transfer CPI. The `authority` must also sign the transaction; the Ethereum signature supplements that check.
- `authority_transfer` moves tokens with only the Solana authority's signature.
- Both transfer handlers use `transfer_checked` through `anchor_spl::token_interface`, so the program works against the Classic Token Program and the Token Extensions Program.
- Tokens are held by a token account owned by a PDA derived from the user account's address; the program signs the CPI with that PDA.

## Setup

From this directory (`tokens/external-delegate-token-master/anchor/`):

```bash
cargo build-sbf
```

Prerequisites: [Agave](https://docs.anza.xyz/) CLI (version in `Anchor.toml` `[toolchain]`) and [Anchor](https://www.anchor-lang.com/docs).

## Testing

Tests run in-process with [LiteSVM](https://www.anchor-lang.com/docs/testing/litesvm). No local validator. Build first so `target/deploy/external_delegate_token_master.so` exists, then:

```bash
cargo test
```

The tests sign real transfer authorizations with a fixed secp256k1 key, send transactions, and assert token balances and nonce state, including the replay, wrong-amount, wrong-recipient, and wrong-authority failure paths.

## Usage

Read the program source under `programs/` and `Anchor.toml` for the program ID. For deployment, use `anchor build && anchor deploy` against your target cluster.
