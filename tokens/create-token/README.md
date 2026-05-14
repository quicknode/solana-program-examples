# Create a Token

Create a token on Solana with metadata such as a symbol and an icon.

All fungible assets and NFTs on Solana are tokens. They follow the [Classic Token Program](https://solana.com/docs/terminology#token-program) standard (similar in spirit to ERC-20), or the newer [Token Extensions](https://solana.com/docs/terminology#token-extensions-program) standard.

```text
Typical fungible tokens : 9 decimals
NFTs                    : 0 decimals
```

## How decimals work

For a token JOE with 9 decimals:

```text
1 JOE = quantity * 10^(-decimals) = 1 * 10^(-9) = 0.000000001
```

## Mint and metadata

A token is represented [onchain](https://solana.com/docs/terminology#onchain) by a **[Mint Account](https://solana.com/docs/terminology#token-mint)**:

```typescript
{
    isInitialized,
    supply,             // Current supply of this mint
    decimals,           // Number of decimals
    mintAuthority,      // Account that can authorise minting
    freezeAuthority,    // Account that can authorise freezing
}
```

Metadata about a mint — name, symbol, image URI — lives in a separate **Metadata [Account](https://solana.com/docs/terminology#account)**:

```typescript
{
    title,
    symbol,
    uri,                // URI to the hosted image / off-asset metadata
}
```

> Metaplex is the de facto standard for token metadata on Solana with the Classic Token Program. The [Metaplex Token Metadata Program](https://docs.metaplex.com/) creates these metadata accounts.
>
> Tokens using the Token Extensions metadata extension store metadata directly on the mint and don't need a separate Metaplex account.

## Steps to create a token

1. Create an account for the mint.
2. Initialize that account as a Mint Account.
3. Create a metadata account associated with the mint.
