# Token Minter

Minting tokens is conceptually straightforward. The subtle part is understanding how Solana tracks per-user token balances.

Every account on Solana tracks its own balance of SOL. It can't possibly also track its own balance of every token on the network. Instead, token balances are held in separate accounts that are specific to a given mint and a given owner. These are called **Associated Token Accounts (ATAs)**.

To know what someone's balance of token JOE is, you would:

1. Create the JOE mint.
2. Create an Associated Token Account for the user's wallet, scoped to the JOE mint.
3. Mint or transfer JOE to that Associated Token Account.

You can think of Associated Token Accounts as per-(mint, wallet) counters: "here is the balance of this mint for this wallet".
