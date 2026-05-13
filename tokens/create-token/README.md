# Create an SPL Token

Create an SPL Token on Solana with metadata such as a symbol and an icon.

All tokens on Solana — including NFTs — are SPL Tokens. They follow the SPL Token standard (similar in spirit to ERC-20).

```text
Default SPL Tokens : 9 decimals
NFTs               : 0 decimals
```

## How decimals work

For a token JOE with 9 decimals:

```text
1 JOE = quantity * 10^(-decimals) = 1 * 10^(-9) = 0.000000001
```

## Mint and metadata

An SPL Token is represented onchain by a **Mint Account**:

```typescript
{
    isInitialized,
    supply,             // Current supply of this mint
    decimals,           // Number of decimals
    mintAuthority,      // Account that can authorise minting
    freezeAuthority,    // Account that can authorise freezing
}
```

Metadata about a mint — name, symbol, image URI — lives in a separate **Metadata Account**:

```typescript
{
    title,
    symbol,
    uri,                // URI to the hosted image / off-asset metadata
}
```

> Metaplex is the de facto standard for SPL Token metadata on Solana. The [Metaplex Token Metadata Program](https://docs.metaplex.com/) is what creates these metadata accounts.

## Steps to create an SPL Token

1. Create an account for the mint.
2. Initialize that account as a Mint Account.
3. Create a metadata account associated with the mint.
