# cNFT Vault (Quasar)

Hold compressed NFTs in a PDA vault and let the stored vault authority withdraw them.

See also: the [repository catalog](../../../README.md).

## Authority model

Deposits are plain Bubblegum transfers to the **vault PDA** (seeds `["cNFT-vault"]`); no program instruction runs on deposit. Because of that, withdraw authorization is per-vault, not per-deposit: `initialize_vault` creates the vault PDA as a `Vault` state account and stores the signer as its **authority**. Both withdraw handlers require that stored authority as a `Signer` (`has_one(authority)`) and reject any other signer with `VaultError::InvalidWithdrawAuthority` before the Bubblegum CPI runs. The same PDA doubles as the Bubblegum leaf owner and signs the transfer CPIs via `invoke_signed`. The seeds, state layout, and error codes match the [Anchor](../anchor/) twin.

Three handlers:

- `initialize_vault` - creates the vault PDA and stores the withdraw authority.
- `withdraw_cnft` - withdraws one cNFT to a recipient chosen by the authority.
- `withdraw_two_cnfts` - withdraws two cNFTs (possibly from different trees) in a single transaction. The client passes `proof_1_length` and `proof_2_length` to split the proof accounts between the two Bubblegum transfers; the handler rejects lengths that do not add up to the supplied proof accounts with `VaultError::ProofLengthMismatch`.

The vault is global to the program deployment: there is one vault PDA with one authority, so anyone who deposits a cNFT is entrusting it to that authority.

## Setup

From `compression/cnft-vault/quasar/`:

```bash
quasar build
```

Prerequisites: [Quasar](https://quasar-lang.com/docs) CLI and [Agave](https://docs.anza.xyz/) toolchain (see `Quasar.toml`).

## Testing

A QuasarSVM integration suite lives in `src/tests.rs`. It loads the same mainnet-dumped fixture binaries as the Anchor twin (Bubblegum, SPL Account Compression, SPL Noop, from `../anchor/tests/fixtures/`), creates a Bubblegum tree, mints cNFTs to the vault PDA, and exercises the withdraw handlers end to end. The suite covers authority withdraws (single and two-cNFT), rejection of non-authority signers, stale-root replays, and out-of-range proof lengths.

```bash
quasar build
quasar test
```

## Usage

Read `src/` and `Quasar.toml`. Compare with the [Anchor](../anchor/) variant in the same example.
