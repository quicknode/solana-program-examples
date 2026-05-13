# Custom Instruction Data

Pass your own custom instruction data to a program. The data must be serialized in a format the Solana runtime can read — typically via the `borsh` crate on both the client and program sides.

- **For `native`:** add `borsh` and `borsh-derive` to `Cargo.toml` so you can mark a struct as serializable.
- **For Anchor:** the framework handles serialization for you via the IDL.
