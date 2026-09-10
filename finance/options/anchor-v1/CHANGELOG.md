# Changelog

## 2026-09-10

The `Market` account is now the token authority of both vaults and signs
every transfer out of them with its own seeds. The separate dataless signer
PDA, the second bump `Market` stored for it, and the extra account
`initialize_market`, `cancel_option`, `exercise_option`, `collect_proceeds`,
`reclaim_collateral` and `collect_fees` took for it are gone.

## 2026-09-04

Initial version: a fully collateralized, physically settled options venue.
A writer posts the whole obligation (the underlying for a call, the strike
in the quote token for a put) and lists an option at a premium; a buyer pays the
premium and becomes the holder; the holder may exercise before expiry; after
expiry the writer reclaims the collateral. Eight instruction handlers
(`initialize_market`, `write_option`, `buy_option`, `cancel_option`,
`exercise_option`, `collect_proceeds`, `reclaim_collateral`,
`collect_fees`), a custody ledger on the market account asserted after every
transfer, and a LiteSVM suite covering both kinds and all three exits.
