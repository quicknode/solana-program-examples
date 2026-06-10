# External Delegate Token Master

A program that lets an **external delegate**, identified by an Ethereum address, authorize token transfers out of a program-controlled vault using a secp256k1 signature, without that delegate ever holding a Solana keypair.

Two builds of the same program live here: [anchor/](anchor/) and [quasar/](quasar/). They share the same state layout semantics, the same signed-message format, and the same checks, so a client written against one works against the other.

## How it works

Each user creates a **user account** (`initialize` instruction handler) storing three fields:

- `authority`: the Solana wallet that owns the user account. Every instruction requires this wallet as a signer.
- `ethereum_address`: a 20-byte Ethereum address set later via `set_ethereum_address`. The delegate's secp256k1 key hashes to this address.
- `nonce`: a strictly increasing counter, starting at zero, consumed by each signature-authorized transfer.

Tokens sit in a token account owned by a **user PDA** derived from the user account's address. The program signs transfer CPIs with this PDA. There are two ways to move tokens, both using `transfer_checked` so the mint and decimals are verified in the CPI:

- `authority_transfer`: the Solana authority signs the transaction directly.
- `transfer_tokens`: the Solana authority signs the transaction AND presents a 65-byte recoverable secp256k1 signature from the delegate. The signature supplements the authority check, it does not replace it.

## Signed message format

The program reconstructs the signed message onchain. The delegate signs the keccak256 hash of this 112-byte preimage:

- program id (32 bytes)
- user account address (32 bytes)
- amount in minor units (8 bytes, little-endian u64)
- recipient token account address (32 bytes)
- nonce (8 bytes, little-endian u64)

The signature is `r || s || recovery id` (65 bytes), over the 32-byte keccak hash directly.

## Nonce semantics

The hash commits to the user account's current `nonce`. On every successful `transfer_tokens` the program increments the stored nonce before invoking the transfer CPI, so:

- each signature authorizes exactly one execution; replaying it fails because the reconstructed message changes,
- a signature over a different amount or recipient fails verification,
- signatures cannot be transplanted between user accounts or programs, because the user account address and program id are part of the hash.

## Testing

Each variant has in-process SVM tests that initialize a user account with a fixed secp256k1 test key, sign real transfer authorizations, send transactions, and assert token balances and nonce state, including the replay, wrong-amount, wrong-recipient, and wrong-authority failure paths.

- Anchor variant: from [anchor/](anchor/), run `cargo build-sbf` then `cargo test` (LiteSVM).
- Quasar variant: from [quasar/](quasar/), run `quasar build` then `quasar test` (QuasarSVM).
