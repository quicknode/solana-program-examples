# Anchor Program

```bash
anchor build
anchor deploy
```

Copy the **program ID** from the output logs and paste it into `Anchor.toml` and `lib.rs`. Then rebuild, redeploy, and run the tests:

```bash
anchor build
anchor deploy
pnpm install
pnpm test
```
