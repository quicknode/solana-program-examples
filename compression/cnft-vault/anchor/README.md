# cNFT Vault

> [!NOTE]
> This is the **Anchor v2** copy of this example. Every `anchor` command on this page
> needs the v2 CLI: `cargo install anchor-cli --version 2.0.0-rc.1 --locked` (avm has
> no prebuilt binary for this pre-release). The Anchor v1 version of this example is in
> [`../anchor-v1`](../anchor-v1/).

Example code for working with Metaplex compressed NFTs (cNFTs) inside Solana [Anchor](https://solana.com/docs/terminology#anchor) [programs](https://solana.com/docs/terminology#program).

The program keeps a PDA-owned vault. You send cNFTs to the vault, then the vault authority withdraws them via the program's [instruction handlers](https://solana.com/docs/terminology#instruction-handler).

## Authority model

Deposits are plain Bubblegum transfers to the **vault PDA** (seeds `["cNFT-vault"]`); no program instruction runs on deposit. Because of that, withdraw authorization is per-vault, not per-deposit: `initialize_vault` creates the vault PDA as a `Vault` state account and stores the signer as its **authority**. Both withdraw handlers require that stored authority as a `Signer` (`address = vault.authority`) and reject any other signer with `VaultError::InvalidWithdrawAuthority` before the Bubblegum CPI runs. The same PDA doubles as the Bubblegum leaf owner and signs the transfer CPIs via `invoke_signed`.

Three handlers:

- `initialize_vault` - creates the vault PDA and stores the withdraw authority.
- `withdraw_cnft` - withdraws one cNFT to a recipient chosen by the authority.
- `withdraw_two_cnfts` - withdraws two cNFTs (possibly from different trees) in a single transaction. The client passes `proof_1_length` and `proof_2_length` to split the proof accounts between the two Bubblegum transfers; the handler rejects lengths that do not add up to the supplied proof accounts with `VaultError::ProofLengthMismatch`.

Use this as a reference for working with cNFTs in your own programs.

## Components

- `programs/cnft-vault/` - the Anchor program.

## Testing

A Rust [LiteSVM](https://www.anchor-lang.com/docs/testing/litesvm) integration suite lives in `programs/cnft-vault/tests/`. It loads mainnet-dumped fixture binaries for Bubblegum, SPL Account Compression, and SPL Noop from `tests/fixtures/` (see the README there), so the CPIs run against the real programs in-process. The suite covers authority withdraws (single and two-cNFT), rejection of non-authority signers, stale-root replays, and out-of-range proof lengths.

```bash
cargo build-sbf
cargo test
```

## Deployment

The program ID declared in [`programs/cnft-vault/src/lib.rs`](programs/cnft-vault/src/lib.rs) is `Fd4iwpPWaCU8BNwGQGtvvrcvG4Tfizq3RgLm8YLBJX6D`. Whether this address is currently deployed on any cluster is not tracked in this repo - verify with `solana program show <id>` against the cluster you care about.

To deploy your own copy, change the program ID in `lib.rs` and `Anchor.toml`, then run `anchor build && anchor deploy`.

## Limitations

This is a reference implementation and is not optimized for compute. The vault is global to the program deployment: there is one vault PDA with one authority, so anyone who deposits a cNFT is entrusting it to that authority.

## Further resources

A video walkthrough is available on [Solandy's YouTube channel](https://youtu.be/qzr-q_E7H0M).
