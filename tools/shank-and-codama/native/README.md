# Shank and Codama

[Shank](https://github.com/metaplex-foundation/shank) lets a **native** Solana
[program](https://solana.com/docs/terminology#program) export an IDL the same
way [Anchor](https://solana.com/docs/terminology#anchor) does. Once you have an
IDL, [Codama](https://github.com/codama-idl/codama) turns it into a typed client
in the language of your choice.

This example is a small "car rental service" program. It is annotated with Shank
macros, Shank extracts the IDL, and Codama renders a TypeScript client
(`@solana/kit`-based) from that IDL. An in-process [LiteSVM](https://github.com/litesvm/litesvm)
test then drives the program through the generated client - no validator or
devnet required, so it runs in CI.

## Shank

[Shank](https://github.com/metaplex-foundation/shank) is a set of Rust derive
macros plus a CLI that generates an IDL for your program.

Mark a struct as an [account](https://solana.com/docs/terminology#account):

```rust
#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, ShankAccount)]
pub struct Car {
    pub year: u16,
    pub make: String,
    pub model: String,
}
```

Mark an enum as your [instruction](https://solana.com/docs/terminology#instruction) set,
using `#[account(...)]` attributes to describe each instruction's accounts:

```rust
#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, ShankInstruction)]
pub enum CarRentalServiceInstruction {
    #[account(0, writable, name = "car_account", desc = "The account that will represent the Car being created")]
    #[account(1, writable, name = "payer", desc = "Fee payer")]
    #[account(2, name = "system_program", desc = "The System Program")]
    AddCar(AddCarArgs),
    // ...
}
```

> Shank needs `declare_id!` in your program for the IDL generation to work:
>
> ```rust
> declare_id!("8avNGHVXDwsELJaWMSoUZ44CirQd4zyU9Ez4ZmP4jNjZ");
> ```

Install the CLI and generate the IDL:

```bash
cargo install shank-cli
pnpm generate-idl   # runs: shank idl --crate-root ./program --out-dir ./program/idl
```

The IDL lands in `program/idl/car_rental_service.json` (committed to the repo so
the client can be regenerated without the Rust CLI). Its `metadata.origin` is
`"shank"`, and each instruction carries an explicit single-byte (`u8`)
`discriminant` - this is what distinguishes a Shank IDL from an Anchor IDL.

### A note on PDAs and `#[seeds(...)]`

Shank's `#[seeds(...)]` attribute is not used here: on Shank 0.4.x its PDA
code-generation produces unparsable tokens and fails to compile, and the seeds
are not emitted into the IDL either. This example instead keeps PDA derivation
explicit in `program/src/state/mod.rs` (`Car::find_pda`, `RentalOrder::find_pda`).
`ShankAccount` is still used - it is what tells Shank to include the account
layout in the IDL.

## Codama

[Codama](https://github.com/codama-idl/codama) reads an IDL and renders a client.
It understands Shank IDLs out of the box.

Install the pieces used here:

```bash
pnpm add codama @codama/nodes-from-anchor @codama/renderers-js @solana/kit
```

The generator script ([`codama.ts`](./codama.ts)) reads the Shank IDL, sets its
`origin` to `"shank"` so the `u8` discriminants are honoured, builds a Codama
root node, and renders a TypeScript client:

```ts
import { rootNodeFromAnchor } from "@codama/nodes-from-anchor";
import { renderVisitor } from "@codama/renderers-js";
import { createFromRoot } from "codama";

const idl = JSON.parse(readFileSync(idlPath, "utf-8"));
const codama = createFromRoot(
  rootNodeFromAnchor({ ...idl, metadata: { ...idl.metadata, origin: "shank" } }),
);
await codama.accept(renderVisitor(outDir, { deleteFolderBeforeRendering: true }));
```

> Codama also ships `@codama/renderers-rust` if you want a Rust client instead of
> a TypeScript one - swap `renderVisitor` from `@codama/renderers-js` for the Rust
> renderer.

Generate the client:

```bash
pnpm generate-client
```

The generated TypeScript client lands in `tests/generated/`.

## Build and test

```bash
pnpm install
pnpm build            # cargo build-sbf -> program/target/so/car_rental_service.so
pnpm build-and-test   # build, regenerate the client, then run the LiteSVM test
```

The test ([`tests/test.ts`](./tests/test.ts)) loads the compiled `.so` into a
[LiteSVM](https://github.com/litesvm/litesvm) instance and drives the full
rental lifecycle (`add_car`, `book_rental`, `pick_up_car`, `return_car`)
through the generated client, asserting on the resulting onchain account
state. It also asserts the program's account validation: a payer that did not
sign, a rental account owned by the wrong program, and an out-of-order status
transition (returning a car that was never picked up) are all rejected with
the named errors from `program/src/error.rs`.
