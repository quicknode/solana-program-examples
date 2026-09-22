# Changelog

## 2026-09-10

The `Market` account is now the token authority of both vaults and signs
every transfer out of them with its own seeds. The separate dataless signer
PDA, the second bump `Market` stored for it, and the extra account
`initialize_market`, `cancel_option`, `exercise_option`, `collect_proceeds`,
`reclaim_collateral` and `collect_fees` took for it are gone.

## 2026-09-04

Initial version: Quasar port of the options example, matching the Anchor
sibling's design and math. `kind` and `status` are `u8` constants, the
writer's premium account is bound by owner and mint in `buy_option`, and
every token account must exist before it is used.
