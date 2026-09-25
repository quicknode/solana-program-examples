# Anchor Program

> [!NOTE]
> This is the **Anchor v1** copy of this example, on Anchor 1.2.0, the current
> stable Anchor release. Every `anchor` command on this page needs the v1 CLI:
> `avm install 1.2.0 && avm use 1.2.0`. The Anchor v2 version of this example is in
> [`../anchor`](../anchor/).

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
