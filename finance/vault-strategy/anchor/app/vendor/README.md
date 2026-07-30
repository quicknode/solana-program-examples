# Vendored dependencies

## `kit-plugin-pyth`

A copy of [`kit-plugin-pyth`](https://github.com/quicknode/solana-kit-plugins) from
`quicknode/solana-kit-plugins`, referenced through a `file:` dependency.

**Why it is copied rather than installed:** the plugin repo is not public yet, so the
package cannot be resolved from npm. Replace this directory with a normal dependency
once it publishes — nothing in the app imports it by path, only by package name, so the
swap is a `package.json` edit.

**Local changes**, which belong upstream and should be contributed back before this copy
is deleted:

- `parsePythPriceUpdateV2Data` / `getPythPriceUpdateV2` — decode pull-oracle
  `PriceUpdateV2` accounts. The plugin could already *create* these accounts via
  `postPythPriceUpdate`, but had no way to read one back: `getPythOnchainPrice` decodes
  the legacy push-oracle layout, which starts with a magic number rather than an Anchor
  discriminator, so it returns `null` for a `PriceUpdateV2` account. The vault-strategy
  program reads `PriceUpdateV2`, so the app needs the pull-oracle decoder.
- The decoder returns raw integers rather than floats. The program does integer math on
  the published price, and the app has to reach the same net asset value it does.
- The `@solana/kit` peer range was widened to admit v7.
