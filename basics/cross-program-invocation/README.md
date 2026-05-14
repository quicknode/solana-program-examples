# Cross-Program Invocation (CPI)

A cross-program invocation is calling one program from another. You use CPIs when your program needs to compose with other onchain programs to do its work.

Whether a given operation should be done via a CPI or via separate RPC calls from the client is a design choice. The main reason to use a CPI is a **dependent operation** that must happen atomically with the rest of your logic.

Consider this sequence in a token mint program:

1. Create and initialize the mint.
2. Create a metadata account for the mint.
3. Create and initialize a user's token account for the mint.
4. Mint some tokens to the user's token account.

You cannot create a metadata account without first having the mint. Once you decide that steps 1 and 4 must be onchain, the only sensible option is to also do steps 2 and 3 onchain — you cannot pause a program mid-flight to let the client do work.

## Native setup notes

With the `native` implementation there is a small bit of setup to import one crate into another inside a Cargo workspace.

A Solana program needs exactly one entry point, so a program that depends on another program must disable the other program's entry point. This is done with Cargo `[features]`.

In the `lever` crate's `Cargo.toml`:

```toml
[features]
no-entrypoint = []
```

In this example each crate also defines a `cpi` feature that depends on `no-entrypoint`, so callers can pick the more descriptive name:

```toml
[features]
no-entrypoint = []
cpi = ["no-entrypoint"]
```

Then, in the `hand` crate, import `lever` with the `cpi` feature enabled:

```toml
[dependencies]
cross-program-invocatio-native-lever = { path = "../lever", features = ["cpi"] }
```

In the `lever` crate, gate the `entrypoint!` macro on the `no-entrypoint` feature being absent:

```rust
#[cfg(not(feature = "no-entrypoint"))]
entrypoint!(process_instruction);
```

This means `lever`'s entrypoint is compiled away when it's pulled in as a dependency, leaving only `hand`'s entrypoint in the final binary.

See the [Features chapter of the Cargo Book](https://doc.rust-lang.org/cargo/reference/features.html) for more on Cargo features.

## The example

<img src="istockphoto-1303616086-612x612.jpeg" alt="lever" width="128" align="center"/>

The `hand` program's `pull_lever` instruction handler does a CPI into the `lever` program's `switch_power` instruction handler. Pull the lever, switch the power.
