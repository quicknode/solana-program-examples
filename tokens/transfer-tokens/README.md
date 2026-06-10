# Transfer Tokens

Like minting, token transfers happen between [Associated Token Accounts](https://solana.com/docs/terminology#associated-token-account-ata).

Use the token program's `transfer_checked` [instruction handler](https://solana.com/docs/terminology#instruction-handler) to move tokens, given the appropriate permissions. `transfer_checked` carries the mint and decimals through the CPI, so a wrong-mint or wrong-decimals account fails the CPI instead of silently moving the wrong quantity. Amounts are passed in minor units, the raw integer the token program operates on.

See [Token Minter](../token-minter) and [NFT Minter](../nft-minter) for more on Associated Token Accounts.
