# NFT Minter

Minting NFTs is the same as [minting any token on Solana](../spl-token-minter/), with one extra step at the end.

When you mint tokens, you can in most cases continue to mint more later, growing the supply. An NFT is supposed to have a supply of **one**, so no more can ever be minted.

The way to do that is to remove the mint authority from the mint:

> The Mint Authority is the account allowed to mint new tokens into supply.

Setting the mint authority to `null` permanently disables minting. **This is irreversible.**

You can do this manually, or use Metaplex to mark the NFT as a Limited Edition. When you use an Edition — such as a Master Edition — for your NFT, you get extra Metaplex metadata, and the mint authority is delegated to the Master Edition account. That delegation effectively disables future minting. Be sure you understand the trade-offs of letting the Master Edition account hold the mint authority instead of setting it permanently to `null`.
