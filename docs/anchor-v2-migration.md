# Porting an Anchor program from v1 to v2.0.0-rc.1

Anchor v2 is a ground-up rewrite, not a version bump. The crate is `no_std` and
built on pinocchio, the account model is static-scoped, borsh is replaced by
wincode, and accounts are zero-copy by default. This is the list of every
difference that came up porting the examples in this repository, in rough order
of how often it bites.

The single most important one is **[borrows held across CPIs](#borrows-held-across-cpis-are-what-tests-catch)**:
it is the only rule here that the compiler will not catch for you.

Every difference below has a worked example in this repository. Each Anchor example keeps
both versions side by side: `<example>/anchor-v1/` is the program on 1.1.2 and
`<example>/anchor/` is the same program on 2.0.0-rc.1, so diffing the two directories shows
any rule here applied to real code. Both are built and tested in CI, by the Anchor v1 and
Anchor v2 workflows respectively.

## Manifest Changes Every Crate Needs

- `anchor-lang` and `anchor-spl` both move from `1.1.2` to `2.0.0-rc.1`.
- `wincode = { version = "0.5", features = ["derive"] }` is new.
- `features = ["init-if-needed"]` goes away. There is no such feature, and the
  constraint is always available.
- `"anchor-spl/idl-build"` comes out of the `idl-build` feature list, because
  anchor-spl has no `idl-build` feature.
- anchor-spl's features are `guardrails` (on by default) and `metadata`. The
  Token Extensions modules are unconditional.

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

## Renamed Accessors and Handler Signatures

- `Context<T>` becomes `&mut Context<T>`, and the user `'info` lifetime is gone:
  `Context<'info, T<'info>>` is `Context<T>`.
- `Signer<'info>` and `Program<'info, T>` are `Signer` and `Program<T>`.
  `Interface<'info, TokenInterface>` is the one wrapper that keeps a lifetime,
  as `Interface<'static, TokenInterface>`.
- `Pubkey` is `Address`. `AccountInfo<'info>` is `AccountView`, or
  `UncheckedAccount` inside an `Accounts` struct.
- `.key()` and `.key` become `.address()`, which returns `&Address`.
  Dereference where a value is wanted.
- `.to_account_info()` becomes `.cpi_handle()` or `.cpi_handle_mut()`.
- `ctx.remaining_accounts` is now the method `ctx.remaining_accounts()?`. It is
  fallible, takes `&mut self`, and returns an owned `Vec<AccountView>`, so call
  it before anything borrows `ctx.accounts`.
- `account.owner` is `account.owner()`. `Rent::minimum_balance` is
  `Rent::try_minimum_balance`.
- `account.data.borrow()` is `account.account().try_borrow()?`.
- `set_inner(X { .. })` is `*ctx.accounts.foo = X { .. }`.
- `account.reload()?` is `account.revalidate_after_cpi()?`. Zero-copy reads are
  already live, so nothing is reloaded; the call re-runs the schema checks a CPI
  could have invalidated.
- `Account::<T>::try_from(view)` is `AnchorAccount::load(view)`, or `load_mut`
  (which is unsafe) to write back.
- `.exit(program_id)` is `.exit()`.
- `error!(MyError::X)` is just `MyError::X`. The macro is compat-only. `?`
  converts, and a tail position needs `.into()`.
- `#[instruction(discriminator = X)]` is `#[discrim = X]`, and it takes a
  literal.
- `#[account(zero)]` is `#[account(zeroed)]`, and `#[account(zero_copy(unsafe))]`
  is plain `#[account]`, which is already zero-copy.
- `AccountLoader<'info, T>` with `load()` is `Account<T>`, which derefs straight
  to `T`.
- `#[account(has_one = x)]` on the owning account is `#[account(address =
  owner.x)]` on the **sibling** field.

`has_one` is deprecated rather than removed, but this repository's `rust.yml` runs
`cargo clippy -- -D warnings`, so every remaining use is a **hard CI failure**.
The check moves off the owning account and onto the sibling it names:

```rust
// v1: on `offer`
#[account(mut, close = maker, has_one = maker)]
pub offer: BorshAccount<Offer>,

// v2: the constraint lives on `maker`
#[account(mut, address = offer.maker)]
pub maker: SystemAccount,
```

Dropping the `'info` lifetime from handler signatures is mechanical, but do not
strip `<'a>` from free functions that genuinely use it: a helper returning
`[&'a [u8]; 4]` still needs its parameter.

## Account State Chooses Between Zero-Copy and Borsh

v2's `#[account]` is **zero-copy** and requires a `Pod` layout: no implicit
padding, no `bool` (use `PodBool`), no `String` or `Vec`.

- State holding `String` / `Vec` / enums → `#[account(borsh)]` + `BorshAccount<T>`.
- Fixed-layout state can stay zero-copy but must carry **explicit padding**
  (`u32` + `u8` leaves three bytes, so name them).
- `#[derive(InitSpace)]` and `#[max_len(N)]` work as before on borsh accounts;
  Pod accounts should size with `core::mem::size_of`.

**Default to `#[account(borsh)]` when porting.** v1 accounts were borsh-encoded,
so it reproduces the onchain layout byte-for-byte and existing clients and
tests keep working. Only keep zero-copy where the program deliberately wants it
(a large slab, say), because converting one to borsh defeats its purpose.

v2 has no `Account::try_from(&AccountView)`. To load a borsh account out of
`remaining_accounts` **for writing**, use `AnchorAccount::load_mut`, which is
`unsafe` because the caller has to guarantee no other live `&mut` to the same
data; `exit()` then writes it back:

```rust
let mut order = unsafe { BorshAccount::<Order>::load_mut(*view) }?;
order.filled_quantity += fill;
order.exit()?;
```

To read one without taking ownership of the write path, check the discriminator
and decode the payload yourself:

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
account (a vendored Pyth or Bubblegum type, say) straightforward to declare.

## Borrows Held Across CPIs Are What Tests Catch

**This is the rule tests catch and the compiler does not.**

v2's typed CPI handles make aliasing a compile error: passing one account into
both a writable and a read-only CPI slot will not build.

For **two read-only slots** there is nothing to work around: `cpi_handle()`
takes `&self`, so calling it twice on the same field is fine. Prefer it: on a
data account the wrapper's own `cpi_handle()` also relaxes the runtime borrow
check, which a hand-built handle does not.

When a read-only slot has to coexist with a writable one, copying the
`AccountView` (which is `Copy`) satisfies the compiler. That is correct **only
for accounts that hold no data borrow** (`Signer`, `UncheckedAccount`,
`Program`), or for a data account whose borrow has already been released:

```rust
let payer_view = *ctx.accounts.payer.account();
// ... CpiHandle::readonly(&payer_view) in the read-only slots
```

On a still-borrowed data account the copy passes the compiler and then fails at
runtime with `AccountBorrowFailed`, because `CpiHandle::readonly` keeps the
borrow check on. Use `into_readonly()` instead: `CpiHandleMut` is `Copy`, and
erasing it carries the wrapper's relaxed borrow flag across.

```rust
// one account filling a writable slot and a read-only one
let mint_handle = ctx.accounts.mint_account.cpi_handle_mut();
let authority_handle = mint_handle.into_readonly();
MintTo { mint: mint_handle, to: ..., authority: authority_handle }
```

Take the writable handle **last**: it borrows the field mutably for the rest of
the scope, so any `msg!` or `.address()` on the same account has to come first.

## An Init Constraint Cannot Name the Account Being Initialized

`mint::authority = mint_account` on `mint_account` itself, a PDA that is its
own mint authority, is rejected at macro-expansion time: an SPL `init`
constraint has to name a *sibling* field. The same goes for
`token::authority = <self>`.

Where that idiom is the point of the example, build the account by hand
(`create_account` + `initialize_mint2` / `initialize_account3`) rather than
adding a second field for the same address, which would then trip v2's
duplicate-mutable-account check. `initialize_mint2` and `initialize_account3`
take the authority as an address, so nothing is lost.

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
   That includes the derive's own use of the account after the handler returns:
   `associated_token::authority = event`, `address = event.x` and friends all
   deref it. So the reacquire has to happen before the handler ends.

   It also means the *seed material* has to be read before the release, since
   reading `event.event_id` is a deref. Where a helper needs it, copy the fields
   into a small struct while the borrow is live and pass that, so the ordering is
   enforced by having to construct the struct first. `betting-market`'s
   `EventSigner::new` is this; three handlers there sign a transfer for the event
   PDA and all three would panic if they read `event_id` a line later. Clippy
   pushes the other way here, because passing the fields individually is what
   trips `too_many_arguments` on the helper: bundling them satisfies both.
2. **Release and reacquire must be on the same branch.** Releasing inside
   `if fee > 0` and reacquiring unconditionally re-borrows an account you still
   hold.
3. **A read-only account cannot be reacquired.** There is no read-only
   reacquire. If the derive references the account after the handler, it has to
   be declared `mut`.

Also: asking a read-only account for a writable handle panics, so read
`*account.address()` rather than `account.cpi_handle_mut().address()`.

**On a `Box`ed account, call `to_cpi_handle_mut()` / `to_cpi_handle()`, not
`cpi_handle_mut()` / `cpi_handle()`.** `Box<T>`'s `AnchorAccount` impl supplies
only `account()`, so `cpi_handle_mut()` falls through to the default, which
builds a handle *without* releasing the wrapper's data borrow, so the CPI is then
rejected with `AccountBorrowFailed`. `Box`'s `ToCpiHandleMut` impl does forward
to the inner type, which is where the release lives. It compiles either way,
so this only shows up in a test.

Not every failure of this kind needs a release: a handler that only *reads* a
**read-only** account can take a second shared borrow (`account().try_borrow()`)
where `try_borrow_mut` on a copied view would be rejected.

A **`mut`** data account is different. Loading one sets pinocchio's exclusive
sentinel (`borrow_state == 0`), so *any* `try_borrow()` on it fails. The
wrapper itself reads through `borrow_unchecked`. Three ways out, in order of
preference:

1. **For a Token Extensions Program extension, use anchor-spl's accessor.**
   `TokenInterfaceAccountExtensions::get_extension::<T>()` on an
   `InterfaceAccount<Mint>` or `InterfaceAccount<TokenAccount>` parses the TLV
   through the borrow the wrapper already holds, and checks the account is
   owned by the Token Extensions Program on the way. It is bounded on `Pod`, so
   it covers every
   fixed-size extension but not a variable-length one like `TokenMetadata`.
2. **Drop the `mut`** if the account is not actually written. A mint passed to
   `transfer_checked_with_fee` is read-only: the withheld fee accrues on the
   destination token account.
3. **Declare the field `UncheckedAccount` and load the typed wrapper by hand.**
   `AnchorAccount::load` is safe, runs exactly the validation the derive would
   have run, and registers a *shared* borrow rather than an exclusive one, so
   an ordinary `try_borrow()` still has room:

   ```rust
   /// CHECK: loaded and validated as an `InterfaceAccount<Mint>` below.
   #[account(mut)]
   pub mint_account: UncheckedAccount,
   ...
   let mint = InterfaceAccount::<Mint>::load(*ctx.accounts.mint_account.account())?;
   let buffer = mint.account().try_borrow()?;
   ```

   Keep the `mut`: it is what marks the account writable in the IDL, which the
   client needs for the CPI that writes to it. `cpi_handle_mut()` on an
   `UncheckedAccount` checks the runtime writable flag, not the wrapper's own
   mutability. Drop the loaded wrapper before the CPI so its shared borrow is
   released. This is what `token-extensions/metadata` does to reach
   `TokenMetadata`, which `get_extension` cannot.

`unsafe { account.account().borrow_unchecked() }`, reading through the
exclusive borrow you already hold, is what `get_extension` does internally, and
it is sound whenever the instruction holds that borrow for its whole duration
and hands out no second one. It is nonetheless not needed anywhere in this
repository's Anchor programs, and it should stay that way: one of the three
options above has covered every case so far.

## Where `unsafe` Is Still Unavoidable

Three Anchor programs read the `LastRestartSlot` sysvar, which pinocchio has no
typed accessor for. Call **`pinocchio::sysvars::get_sysvar(&mut buf, &id, 0)`**
rather than the `sol_get_sysvar` syscall directly: it is a safe wrapper, and
offchain it is a no-op that leaves the buffer zeroed, so IDL and client builds
read "the cluster has never restarted" without a `cfg` split of their own.

After that, two kinds of `unsafe` are left in the Anchor programs, and neither
has a safe equivalent in v2:

- **`transfer-hook/*/anchor/…/entrypoint.rs`**: `pub unsafe extern "C" fn
  entrypoint` is the loader ABI. These programs claim the entrypoint themselves
  to remap an SPL interface discriminator, so the `unsafe` the `#[program]`
  macro would have hidden is written out.
- **`order-book/…/place_order.rs`**: `AnchorAccount::load_mut` is an `unsafe fn`
  on the trait, and it is the only way to get a writable typed wrapper for an
  account arriving through `remaining_accounts`. `BorshAccount`'s implementation
  borrows through the checked `try_borrow_mut`, so a duplicate fails with
  `AccountBorrowFailed` rather than aliasing.

Any lint that bans `unsafe` repository-wide has to exempt those, plus the
Quasar and pinocchio programs, where raw zero-copy access is the point of the
example rather than an escape hatch.

## Discriminators, and Programs That Implement an Interface

By default a handler still dispatches on `sha256("global:<name>")[..8]`, so
existing clients and tests keep working. What changed is the override:

- `#[interface(...)]` and the `interface-instructions` feature are **gone**.
- `#[discrim = N]` on an executable `#[program]` takes a **single byte**, and
  it is all-or-nothing: if one handler has it, every handler needs one.
- `#[program(interface, program_id = ID)]` accepts arbitrary discriminator
  bytes, but it declares an interface for *other* programs to CPI into. It
  generates a CPI client and no dispatch, so the crate builds to a ~900-byte
  object with no `entrypoint` symbol and fails to load with
  `ProgramLoad("Entrypoint out of bounds")`.

That leaves no direct way to write a program that answers to a foreign
eight-byte discriminator, an SPL transfer hook's `Execute`, say. The
transfer-hook examples here handle it by taking the entrypoint over: the crate
sets `default = ["no-entrypoint"]`, which makes anchor export its dispatch as
`__anchor_dispatch` instead of claiming `entrypoint`, and `src/entrypoint.rs`
claims `entrypoint` itself, swaps the interface discriminator for the matching
handler's, and delegates. The payload behind the discriminator is unchanged, so
nothing else has to be replicated. With `no-entrypoint` set, the crate also has
to invoke `pinocchio::default_allocator!()` and
`pinocchio::default_panic_handler!()` itself: anchor only emits those on the
path where it owns the entrypoint.

## Duplicate Mutable Accounts Are Rejected

v2 rejects an account that appears in more than one declared slot when any of
those slots is mutable, with `ConstraintDuplicateMutableAccount`, custom error
2040. `#[account(unsafe(dup))]` opts a slot out; it implies `mut`, so it
replaces the `mut` rather than joining it.

The catch: the walker flags **both** indices of a duplicate, so marking only
the second one still leaves the first intersecting the mutable mask. Every slot
that can legitimately alias needs the constraint, including a `payer`.

Why the rule exists: v1 deserialized each `Account<T>` into an owned copy and
serialized it back at the end of the instruction. Two slots over one account
meant two independent copies, and the second write-back silently clobbered the
first, the classic self-transfer bug, where debiting one copy and crediting
the other and writing both leaves the balance higher than it started. v2
accounts are zero-copy views into the runtime's buffer, so two mutable wrappers
over one account would be two `&mut` to the same bytes. Hence the loader
rejects rather than warns.

The `unsafe` is not decoration, so check two things before reaching for it:

1. **That the accounts can actually alias.** If no caller ever passes the same
   address twice, the constraint disables a live check for nothing. Leave it
   off. `transfer-tokens`' `transfer` is this case: its test funds a fresh
   `Keypair` as the recipient.
2. **That the aliasing slots hold no deserialized state.** `Signer`,
   `SystemAccount` and `UncheckedAccount` carry no copy to write back, so
   there is no lost update to worry about. The two `mint` instructions here are
   this case: minting to yourself makes `mint_authority` and `recipient` the
   same `Signer`, and the duplication comes from the caller rather than the
   program, so refusing it would make an ordinary mint fail with 2040. Two
   aliasing `Account<T>` slots are what the check is *for*, and the fix there
   is to restructure the accounts so only one of them exists.

## anchor-spl Moved Its Types and Dropped Its Features

`Mint` moved from `anchor_spl::token` to `anchor_spl::mint`, and the namespaced
constraints (`mint::decimals`, `token::authority`, …) expand to paths rooted at
those modules, so `mint` / `token` must be nameable in the file that uses them.

SPL account fields are behind accessors now that the structs are Pod:
`.amount()`, `.decimals()`, `.supply()`, `.mint()`, `.owner()`.

Init constraints resolve a **sibling account field**, not a pubkey expression or
a field read off another account: `mint::authority = payer`, not
`mint::authority = payer.address()`; `associated_token::mint = mint_to_raise`,
not `= fundraiser.mint_to_raise`.

CPI structs dropped their `token_program_id` / `program_id` slots. The program
comes from the `CpiContext`. `create_metadata_accounts_v3` takes four arguments
(the signer flag moved into the accounts struct, and the optional `rent` account
is gone).

There are **no `extensions::*` constraints**, for init or validation. To create a
mint with a Token Extensions Program extension, do it by hand, which is what
this repository's
`non-transferable` example always did:

```rust
let mint_size = ExtensionType::try_calculate_account_len::<PodMint>(&[
    ExtensionType::MintCloseAuthority,
])?;
// create_account -> <extension>_initialize -> initialize_mint2, in that order
```

Extension initialization must come before `InitializeMint2`.

## v2-Only Primitives Worth Reaching For

These have no v1 equivalent, so a mechanical port never produces them, but they
are often the right answer when a straight translation gets ugly:

- `Slab<H, Item>`: zero-copy header with a dynamic item tail, for ledgers,
  order books and event logs. `Account<T>` is `Slab<T, HeaderOnly>` underneath.
- `PodVec<T, MAX>`: fixed-capacity vec with a `u16` length, stored inline, so
  variable-length data without leaving Pod.
- `PodU64`, `PodI128`, `PodBool` and friends: alignment-1 integer wrappers, so a
  `#[repr(C)]` struct packs with no padding.
- `#[pod_wrapper]`: safe enum-to-Pod conversion that validates the discriminant
  on equality and conversion.
- `Nested<T>`: share one `#[derive(Accounts)]` validation block across
  instructions.
- `#[event(bytemuck)]`: fixed-size events, a discriminator plus one memcpy.
  Plain `#[event]` defaults to wincode, which is borsh-wire-compatible.

If a program was zero-copy in v1 and the port is fighting Pod's rules, the
answer is usually one of the first four rather than moving it to borsh.

### Feature flags

- `guardrails` (default on): runtime safety nets. Dropping it saves ~300 bytes
  and 1–2 CU per account, at the cost of diagnostic panics on misuse.
- `const-rent` (default off): folds `Rent::get()` to a compile-time constant,
  ~85 CU per `create_account`. Burns the rent formula into the binary, so a
  formula change (SIMD-0194) needs a rebuild.
- `compat` (default off): restores v1-shaped `error!`, `err!`, `pubkey!`,
  `debug!`. Useful mid-port; `debug!` heap-allocates through `alloc::format!`.

### Tooling

- `anchor debugger`: TUI stepping through SBF instructions per test.
- `anchor test --profile`: per-test register-trace flamegraphs under
  `target/anchor-v2-profile/`.
- `anchor-v2-testing`: wraps LiteSVM with optional register-trace capture.

## Hand-Built CPIs Take Handles, Not Account Infos

An example that builds its own `Instruction`, because the callee's crate is
not on a compatible Solana version, hands `invoke` a slice of `CpiHandle`
rather than a slice of `AccountInfo`. Four rules, all of them enforced at
runtime rather than by the compiler:

- **The handles are positional.** v2 walks the instruction's account metas and
  binds each one to the next handle in the slice. A handle that does not match
  the meta at its position fails the whole call with `InvalidArgument`, so the
  list has to mirror the metas exactly.
- **The program account is not a handle.** v1 code habitually pushed the callee
  program's `AccountInfo` into the infos vec; in v2 that extra leading entry is
  what breaks the positional match.
- **An account filling two meta slots supplies two handles.** Bubblegum's
  `Transfer` names the same PDA as both `leaf_owner` and `leaf_delegate`, so the
  handle appears twice. Read-only handles make this trivial: `cpi_handle()`
  takes `&self`.
- **Writability has to be at least as strong as the meta.** A writable meta
  needs a writable handle; a read-only meta accepts either. Going the other way,
  `cpi_handle_mut()` on an account not declared `mut` panics outright with
  *"cpi_handle_mut called on a read-only account"*, which surfaces as
  `ProgramFailedToComplete` and a `src/traits.rs` log line.

Mixing the two handle kinds in one vec means converting element-wise, as in
`vec![a.cpi_handle_mut().into(), b.cpi_handle()]`, since a trailing
`.map(CpiHandle::from)` forces every element to be a `CpiHandleMut`.

A `BorshAccount` that signs such a CPI still needs `release_borrow()` first, per
[Borrows held across CPIs](#borrows-held-across-cpis-are-what-tests-catch). When the account is read-only and
nothing reads it after the CPI, there is no reacquire, because `reacquire_borrow_mut`
asserts the account was loaded mutably.

Vendored types that derive `BorshSerialize` over `Address` fields need the
`borsh` feature on `solana-address`; `Address` is pinocchio's re-export of that
crate's type and carries no borsh impls by default. Their `serialize` returns
`io::Error`, which does not convert into `ProgramError`, so map it:

```rust
args.serialize(&mut data)
    .map_err(|_| ProgramError::InvalidInstructionData)?;
```

## Seeds, Sysvars and Cross-Program Types

`seeds` takes a byte array directly and binds it itself: `id.to_le_bytes()`,
not `id.to_le_bytes().as_ref()`, which produces a temporary that dies before the
derive uses it.

pinocchio ships only the `Clock` and `Rent` sysvars. Anything else (this
repository needs `LastRestartSlot`) has to be declared locally and read through
the
`sol_get_sysvar` syscall (`solana-define-syscall`). The Quasar variants of these
same examples carry the identical workaround.

A sibling program's account is an `UncheckedAccount` validated by a constraint:
v2 only generates a `program` marker module from `declare_program!`, not from
`#[program]` in a dependency crate.

Ambiguity to watch for: the prelude exports an `Event` trait, so a state struct
named `Event` that reaches the crate root via glob re-export becomes ambiguous.
Import it explicitly (`use crate::state::Event;`).

## Tests Assert Numeric Error Codes

The test-side surface barely changed, but three things move:

- `anchor_lang::solana_program` has no `system_program` submodule. The real
  module is at the crate root and exposes `ID`, not `id()`.
- `solana_program::pubkey::Pubkey` only exists under the `compat` feature;
  `anchor_lang::Address` is the same 32-byte type.
- `anchor_lang::prelude::Clock` is pinocchio's onchain type. LiteSVM's
  `get_sysvar` / `set_sysvar` want the host-side `solana_clock::Clock`.
- `#[error_code]` no longer generates `From<MyError> for u32`. It makes the enum
  `#[repr(u32)]` and generates only `From<MyError> for anchor_lang::Error`, so a
  test asserting on the wire code writes `my_error as u32 + 6000` (6000 being
  the default offset, overridable with `#[error_code(offset = ...)]`).

Tests that decode account bytes with borsh keep working, because
`BorshConfig` makes wincode's wire format byte-identical, but a Pod account
that grew explicit padding needs that padding mirrored in the test's decode
struct, since `try_from_slice` rejects trailing bytes.

## A Workspace With More Than One Program Needs Per-Crate Builds

`cargo-build-sbf` at an anchor workspace root builds every member in one
invocation, and cargo unifies features across them. A program that depends on a
sibling with `features = ["cpi"]`, which implies `no-entrypoint`, therefore
makes that sibling's *own* `.so` build with `no-entrypoint` too, and in v2 that
exports its dispatch as `__anchor_dispatch` rather than `entrypoint`. The
result loads with `ProgramLoad("Entrypoint out of bounds")`.

`anchor build` builds each program separately and is unaffected. Anything else
driving `cargo-build-sbf` has to do the same.

## Toolchain Versions the Port Depends On

CI installs the CLI with `avm install 2.0.0-rc.1`; it is a pre-release, so it
has to be named explicitly rather than resolved as latest. A v2 CLI cannot build
v1 programs, so that pin can only move once every example is ported.

`anchor idl build` compiles the test targets too, so the `.so` has to exist
first, so run `cargo-build-sbf` before regenerating an IDL.
