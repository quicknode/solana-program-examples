# Favorites

A basic [Anchor](https://solana.com/docs/terminology#anchor) app that uses [PDAs](https://solana.com/docs/terminology#program-derived-address-pda) to store per-user data, and Anchor [account](https://solana.com/docs/terminology#account) constraints to ensure each user can only modify their own data.

Used by the [Solana Professional Education](https://github.com/solana-developers/professional-education) course.

## Usage

Run the tests with `pnpm test` (as configured in `Anchor.toml`). Deploy with `anchor deploy`.
