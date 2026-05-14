# PDA Mint Authority

The same as the [NFT Minter](../nft-minter) example, except the **[mint](https://solana.com/docs/terminology#token-mint) authority** is a [PDA](https://solana.com/docs/terminology#program-derived-address-pda) rather than a system [account](https://solana.com/docs/terminology#account) belonging to the payer.

💡 Notice the use of `invoke_signed` for [CPIs](https://solana.com/docs/terminology#cross-program-invocation-cpi).
