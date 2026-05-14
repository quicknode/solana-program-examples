# Shank and Solita

The Metaplex team built **Shank** and **Solita** so that native Solana [programs](https://solana.com/docs/terminology#program) can have serialization and IDL support similar to [Anchor](https://solana.com/docs/terminology#anchor).

## Shank

[Shank](https://github.com/metaplex-foundation/shank) is a Rust crate that generates an IDL for your program.

Mark a struct as an [account](https://solana.com/docs/terminology#account):

```rust
#[derive(BorshDeserialize, BorshSerialize, Clone, ShankAccount)]
pub struct Car {
    pub year: u16,
    pub make: String,
    pub model: String,
}
```

Mark an enum as your [instruction](https://solana.com/docs/terminology#instruction) set:

```rust
#[derive(BorshDeserialize, BorshSerialize, Clone, ShankInstruction)]
pub enum CarRentalServiceInstruction {
    AddCar(Car),
    BookRental(RentalOrder),
    PickUpCar,
    ReturnCar,
}
```

Install the CLI and generate the IDL:

```bash
cargo install shank-cli
shank idl
```

> Shank needs `declare_id!` in your program for the IDL generation to work:
>
> ```rust
> declare_id!("8avNGHVXDwsELJaWMSoUZ44CirQd4zyU9Ez4ZmP4jNjZ");
> ```

## Solita

[Solita](https://github.com/metaplex-foundation/solita) is the JavaScript SDK generator. It turns your IDL into a TypeScript client.

> Solita works with both Shank IDLs and Anchor IDLs.

Install it:

```bash
pnpm add -D @metaplex-foundation/solita
```

Then add a `.solitarc.js` at the example root:

```javascript
const path = require("node:path");
const programDir = path.join(__dirname, "program");
const idlDir = path.join(programDir, "idl");
const sdkDir = path.join(__dirname, "tests", "generated");
const binaryInstallDir = path.join(__dirname, ".crates");

module.exports = {
    idlGenerator: "shank",
    programName: "car_rental_service",
    idlDir,
    sdkDir,
    binaryInstallDir,
    programDir,
};
```

Generate the client:

```bash
pnpm solita
```

The generated TypeScript lands in `tests/generated/`.
