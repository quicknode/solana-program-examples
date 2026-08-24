# Anchor Program

> [!NOTE]
> This is the **Anchor v2** copy of this example. Every `anchor` command on this page
> needs the v2 CLI: `cargo install anchor-cli --version 2.0.0-rc.1 --locked` (avm has
> no prebuilt binary for this pre-release). The Anchor v1 version of this example is in
> [`../anchor-v1`](../anchor-v1/).

```bash
anchor build
anchor deploy
```

Copy the **[program](https://solana.com/docs/terminology#program) ID** from the output logs and paste it into `Anchor.toml` and `lib.rs`. Then rebuild, redeploy, and run the tests:

```bash
anchor build
anchor deploy
pnpm install
anchor test
```
