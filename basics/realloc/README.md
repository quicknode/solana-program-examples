# Realloc

Resize a Solana [account](https://solana.com/docs/terminology#account) after it has been created — grow or shrink the data it can hold.

## A note on `realloc` vs `resize`

The runtime method `AccountInfo::realloc` has been deprecated in favor of `AccountInfo::resize` ([anchor#4526](https://github.com/solana-foundation/anchor/issues/4526)). New code should call `AccountInfo::resize`.

The [Anchor](https://solana.com/docs/terminology#anchor) account-constraint macros (`#[account(realloc = ..., realloc::payer = ..., realloc::zero = ...)]`) are **not yet renamed** and still use the `realloc` spelling. That is the correct form to use today; track the issue above for any future change.
