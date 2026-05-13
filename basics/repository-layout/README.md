# Recommended Program Layout

A typical layout for a Solana program as it grows in size and starts to need multiple Rust files. Many programs follow this shape.

> You can structure your `src` folder however you like, as long as it follows Cargo's conventions. This layout is shown so that the patterns in other programs are recognizable.

The `native` and `anchor` layouts are similar. The main difference is the `processor.rs` file in the `native` setup — one of the things Anchor abstracts away for you.
