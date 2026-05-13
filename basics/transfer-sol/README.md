# Transfer SOL

A simple example of transferring SOL between two system accounts. SOL can be transferred between many kinds of accounts, not just system accounts (accounts owned by the System Program).

The tests generate a fresh keypair for both the `native` and `anchor` versions. Transferring SOL to the new keypair's address initializes it as a default system account — hence the `/// CHECK` annotation above it in the Anchor example.
