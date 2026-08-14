# Porting an Anchor program from v1 to v2.0.0-rc.1

Anchor v2 is a ground-up rewrite, not a version bump. The crate is `no_std` and
built on pinocchio, the account model is static-scoped, borsh is replaced by
wincode, and accounts are zero-copy by default. This is the list of every
difference that came up porting the examples in this repository, in rough order
of how often it bites.

The single most important one is **[borrows across CPIs](#borrows-across-cpis)**:
it is the only rule here that the compiler will not catch for you.

## Manifest

| v1 | v2 |
|---|---|
| `anchor-lang = "1.1.2"` | `anchor-lang = "2.0.0-rc.1"` |
| `anchor-spl = "1.1.2"` | `anchor-spl = "2.0.0-rc.1"` |
| — | `wincode = { version = "0.5", features = ["derive"] }` |
| `features = ["init-if-needed"]` | no such feature; the constraint is always available |
| `idl-build = [... "anchor-spl/idl-build"]` | anchor-spl has no `idl-build`; drop it |
| `anchor-spl` features `metadata`, `spl-token-interface` | only `guardrails` (default) and `metadata` exist |

Every program crate needs `wincode` as a **direct** dependency: the `#[program]`
macro expands to `wincode` paths for instruction-data (de)serialization.

Programs that put an `Address` in serialized state also need:

```toml
# anchor-lang 2.0.0-rc.1 is built against wincode 0.5, but solana-address 2.7
# moved to wincode 0.6. With both in the graph, `Address`'s wincode impls belong
# to the version the account derives are not using, and every `SchemaRead` /
# `SchemaWrite` bound fails. 2.6.1 is the last release on the 0.5 line.
solana-address = ">=2.6, <2.7"
```

Note the range: a bare `<2.7` lets cargo satisfy the requirement by reusing an
older 1.x already in the graph, which does not fix anything.

## Signatures and types

| v1 | v2 |
|---|---|
| `Context<T>` | `&mut Context<T>` |
| `Context<'info, T<'info>>` | `Context<T>` — no user lifetime |
| `Pubkey` | `Address` |
| `AccountInfo<'info>` | `AccountView` (or `UncheckedAccount` in an `Accounts` struct) |
| `Signer<'info>`, `Program<'info, T>`, … | `Signer`, `Program<T>` — the `'info` lifetime is gone |
| `Interface<'info, TokenInterface>` | `Interface<'static, TokenInterface>` — the one wrapper that keeps a lifetime |
| `.key()` / `.key` | `.address()`, which returns `&Address` |
| `.to_account_info()` | `.cpi_handle()` / `.cpi_handle_mut()` |
| `ctx.remaining_accounts` (field) | `ctx.remaining_accounts()?` — fallible, and takes `&mut self` |
| `account.owner` | `account.owner()` |
| `Rent::minimum_balance` | `Rent::try_minimum_balance` |
| `#[instruction(discriminator = X)]` | `#[discrim = X]`, and it takes a literal |
| `#[account(zero)]` | `#[account(zeroed)]` |
| `#[account(zero_copy(unsafe))]` | `#[account]` — already zero-copy |
| `AccountLoader<'info, T>` + `load()` | `Account<T>`, which derefs straight to `T` |
| `set_inner(X { .. })` | `*ctx.accounts.foo = X { .. }` |
| `error!(MyError::X)` | `MyError::X` (compat-only macro; `?` converts, tail position needs `.into()`) |

Dropping the `'info` lifetime from handler signatures is mechanical, but do not
strip `<'a>` from free functions that genuinely use it — a helper returning
`[&'a [u8]; 4]` still needs its parameter.

## Account backing

v2's `#[account]` is **zero-copy** and requires a `Pod` layout: no implicit
padding, no `bool` (use `PodBool`), no `String` or `Vec`.

- State holding `String` / `Vec` / enums → `#[account(borsh)]` + `BorshAccount<T>`.
- Fixed-layout state can stay zero-copy but must carry **explicit padding**
  (`u32` + `u8` leaves three bytes — name them).
- `#[derive(InitSpace)]` and `#[max_len(N)]` work as before on borsh accounts;
  Pod accounts should size with `core::mem::size_of`.

**Default to `#[account(borsh)]` when porting.** v1 accounts were borsh-encoded,
so it reproduces the on-chain layout byte-for-byte and existing clients and
tests keep working. Only keep zero-copy where the program deliberately wants it
(a large slab, say) — converting such a program to borsh defeats its purpose.

v2 has no `Account::try_from(&AccountView)`. To load a borsh account out of
`remaining_accounts`, check the discriminator and read the payload yourself:

```rust
let data = account.try_borrow()?;
let disc_len = <T as anchor_lang::Discriminator>::DISCRIMINATOR.len();
require!(
    data.len() > disc_len
        && &data[..disc_len] == <T as anchor_lang::Discriminator>::DISCRIMINATOR,
    MyError::BadAccount
);
let mut payload = &data[disc_len..];
<T as wincode::SchemaRead<anchor_lang::BorshConfig>>::get(&mut payload)
    .map_err(|_| MyError::BadAccount)?
```

`Owner` is a const in v2 (`const OWNER: Address`), which makes a foreign-owned
account — a vendored Pyth or Bubblegum type, say — straightforward to declare.

## Borrows across CPIs

**This is the rule tests catch and the compiler does not.**

v2's typed CPI handles make aliasing a compile error: passing one account into
both a writable and a read-only CPI slot will not build. The obvious workaround
— copying the `AccountView`, which is `Copy` — satisfies the compiler, and is
correct **only for accounts that hold no data borrow** (`Signer`,
`UncheckedAccount`, `Program`):

```rust
let payer_view = *ctx.accounts.payer.account();
// ... CpiHandle::readonly(&payer_view) in the read-only slots
```

For a **data** account (`Account`, `BorshAccount`, `InterfaceAccount`) that
copy is not enough. The account holds a live borrow on its buffer, and the
runtime rejects the CPI's own borrow with `AccountBorrowFailed`. Release it
across the call instead:

```rust
ctx.accounts.offer.release_borrow()?;
let offer_view = *ctx.accounts.offer.account();
// ... CPI signed by `offer` ...
ctx.accounts.offer.reacquire_borrow_mut()?;
```

`reacquire_borrow_mut` re-runs the load-time owner and discriminator checks,
because a CPI in the release window could have mutated either.

Three ways this goes wrong:

1. **Dereferencing after release panics** (`account borrow released (closed)`).
   That includes the derive's own use of the account after the handler returns —
   `associated_token::authority = event`, `has_one = event` and friends all
   deref it. So the reacquire has to happen before the handler ends.
2. **Release and reacquire must be on the same branch.** Releasing inside
   `if fee > 0` and reacquiring unconditionally re-borrows an account you still
   hold.
3. **A read-only account cannot be reacquired** — there is no read-only
   reacquire. If the derive references the account after the handler, it has to
   be declared `mut`.

Also: asking a read-only account for a writable handle panics, so read
`*account.address()` rather than `account.cpi_handle_mut().address()`.

## anchor-spl

`Mint` moved from `anchor_spl::token` to `anchor_spl::mint`, and the namespaced
constraints (`mint::decimals`, `token::authority`, …) expand to paths rooted at
those modules, so `mint` / `token` must be nameable in the file that uses them.

SPL account fields are behind accessors now that the structs are Pod:
`.amount()`, `.decimals()`, `.supply()`, `.mint()`, `.owner()`.

Init constraints resolve a **sibling account field**, not a pubkey expression or
a field read off another account: `mint::authority = payer`, not
`mint::authority = payer.address()`; `associated_token::mint = mint_to_raise`,
not `= fundraiser.mint_to_raise`.

CPI structs dropped their `token_program_id` / `program_id` slots — the program
comes from the `CpiContext`. `create_metadata_accounts_v3` takes four arguments
(the signer flag moved into the accounts struct, and the optional `rent` account
is gone).

There are **no `extensions::*` constraints**, for init or validation. To create a
mint with a Token-2022 extension, do it by hand — which is what this repo's
`non-transferable` example always did:

```rust
let mint_size = ExtensionType::try_calculate_account_len::<PodMint>(&[
    ExtensionType::MintCloseAuthority,
])?;
// create_account -> <extension>_initialize -> initialize_mint2, in that order
```

Extension initialization must come before `InitializeMint2`.

## Seeds, sysvars, cross-program types

`seeds` takes a byte array directly and binds it itself — `id.to_le_bytes()`,
not `id.to_le_bytes().as_ref()`, which produces a temporary that dies before the
derive uses it.

pinocchio ships only the `Clock` and `Rent` sysvars. Anything else — this repo
needs `LastRestartSlot` — has to be declared locally and read through the
`sol_get_sysvar` syscall (`solana-define-syscall`). The Quasar variants of these
same examples carry the identical workaround.

A sibling program's account is an `UncheckedAccount` validated by a constraint:
v2 only generates a `program` marker module from `declare_program!`, not from
`#[program]` in a dependency crate.

Ambiguity to watch for: the prelude exports an `Event` trait, so a state struct
named `Event` that reaches the crate root via glob re-export becomes ambiguous.
Import it explicitly (`use crate::state::Event;`).

## Tests

The test-side surface barely changed, but three things move:

- `anchor_lang::solana_program` has no `system_program` submodule. The real
  module is at the crate root and exposes `ID`, not `id()`.
- `solana_program::pubkey::Pubkey` only exists under the `compat` feature;
  `anchor_lang::Address` is the same 32-byte type.
- `anchor_lang::prelude::Clock` is pinocchio's on-chain type. LiteSVM's
  `get_sysvar` / `set_sysvar` want the host-side `solana_clock::Clock`.

Tests that decode account bytes with borsh keep working, because
`BorshConfig` makes wincode's wire format byte-identical — but a Pod account
that grew explicit padding needs that padding mirrored in the test's decode
struct, since `try_from_slice` rejects trailing bytes.

## Toolchain

CI installs the CLI with `avm install 2.0.0-rc.1`; it is a pre-release, so it
has to be named explicitly rather than resolved as latest. A v2 CLI cannot build
v1 programs, so that pin can only move once every example is ported.

`anchor idl build` compiles the test targets too, so the `.so` has to exist
first — run `cargo-build-sbf` before regenerating an IDL.
