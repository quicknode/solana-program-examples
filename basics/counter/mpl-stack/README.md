# Counter: MPL Stack

A Solana-native counter built using the MPL (Metaplex) stack.

## Setup

1. Build the program: `cargo build-sbf`
2. Build the IDL: `shank build`
3. Build the TypeScript SDK: `pnpm solita`
   - Temporary workaround: edit `ts/generated/accounts/Counter.ts` line 58 to
     `const accountInfo = await connection.getAccountInfo(address, { commitment: "confirmed" });`
     so that the tests pass. Future Solita versions will fix this.
4. Run tests: `pnpm test`
