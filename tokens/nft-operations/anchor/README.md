# NFT Operations

Create an NFT collection, mint an NFT, and verify an NFT as part of a collection - all using Metaplex Token Metadata.

## Program setup

The [CPIs](https://solana.com/docs/terminology#cross-program-invocation-cpi) that create metadata [accounts](https://solana.com/docs/terminology#account) and master edition accounts, and that verify NFTs as part of a collection, all target the Metaplex Token Metadata program. The Rust test suite loads a dump of that program from `tests/fixtures/mpl_token_metadata.so` into LiteSVM. To refresh the dump from mainnet, run `prepare.mjs` (requires [zx](https://github.com/google/zx)).

## Create an NFT collection

The accounts needed to create an NFT collection are:

```rust
#[derive(Accounts)]
pub struct CreateCollectionAccountConstraints {
    #[account(mut)]
    pub user: Signer,

    #[account(
        init,
        payer = user,
        mint::decimals = 0,
        mint::authority = mint_authority,
        mint::freeze_authority = mint_authority,
    )]
    pub mint: Account<Mint>,

    #[account(
        seeds = [b"authority"],
        bump,
    )]
    /// CHECK: This account is not initialized and is being used for signing purposes only
    pub mint_authority: UncheckedAccount,

    #[account(mut)]
    /// CHECK: This account will be initialized by the metaplex program
    pub metadata: UncheckedAccount,

    #[account(mut)]
    /// CHECK: This account will be initialized by the metaplex program
    pub master_edition: UncheckedAccount,

    #[account(
        init,
        payer = user,
        associated_token::mint = mint,
        associated_token::authority = user
    )]
    pub destination: Account<TokenAccount>,

    pub system_program: Program<System>,
    pub token_program: Program<Token>,
    pub associated_token_program: Program<AssociatedToken>,
    pub token_metadata_program: Program<Metadata>,
}
```

### Account breakdown

- `user`: the account creating the collection NFT and the owner of the destination [token account](https://solana.com/docs/terminology#token-account).
- `mint`: the collection NFT [mint account](https://solana.com/docs/terminology#token-mint). Initialized with 0 decimals; the mint authority and freeze authority are set to `mint_authority`.
- `mint_authority`: the [PDA](https://solana.com/docs/terminology#program-derived-address-pda) authority used to mint tokens from the collection mint.
- `metadata`: the metadata account of the collection NFT.
- `master_edition`: the master edition account of the collection NFT.
- `destination`: the token account that receives the collection NFT.
- `system_program`: initializes new accounts.
- `token_program` / `associated_token_program`: create new [ATAs](https://solana.com/docs/terminology#associated-token-account-ata) and mint tokens.
- `token_metadata_program`: the MPL Token Metadata program, used to create the metadata and master edition accounts.

The `metadata` and `master_edition` accounts are `UncheckedAccount` because the Metaplex program initializes them during the CPI. If instead we wrote:

```rust
#[account(mut)]
pub metadata: MetadataAccount,
#[account(mut)]
pub master_edition: MasterEditionAccount,
```

the instruction would fail because [Anchor](https://solana.com/docs/terminology#anchor) would expect the accounts to already be initialized.

When an account *is* already initialized (as in the verify-collection flow below), use the specific account types.

### Implementation for `create_collection`

Each [instruction handler](https://solana.com/docs/terminology#instruction-handler) is a free function called from the `#[program]` module in `lib.rs`. The account constraints struct lives in the same file as the handler. The metadata `name`, `symbol`, and `uri` are instruction arguments, validated against the Metaplex limits (32, 10, and 200 bytes) by `validate_metadata_strings`, which returns the named errors `NameTooLong` / `SymbolTooLong` / `UriTooLong` instead of an opaque CPI failure.

```rust
pub fn handle_create_collection(
    accounts: &mut CreateCollectionAccountConstraints,
    bumps: &CreateCollectionAccountConstraintsBumps,
    name: String,
    symbol: String,
    uri: String,
) -> Result<()> {
    validate_metadata_strings(&name, &symbol, &uri)?;

    let seeds = &[&b"authority"[..], &[bumps.mint_authority]];
    let signer_seeds = &[&seeds[..]];

    let cpi_accounts = MintTo {
        mint: accounts.mint.cpi_handle_mut(),
        to: accounts.destination.cpi_handle_mut(),
        authority: accounts.mint_authority.cpi_handle(),
    };
    let cpi_ctx =
        CpiContext::new_with_signer(accounts.token_program.address(), cpi_accounts, signer_seeds);
    mint_to(cpi_ctx, 1)?;
    msg!("Collection NFT minted!");

    let creator = vec![Creator {
        address: *accounts.mint_authority.address(),
        verified: true,
        share: 100,
    }];

    create_metadata_accounts_v3(
        CpiContext::new_with_signer(
            accounts.token_metadata_program.address(),
            CreateMetadataAccountsV3 {
                metadata: accounts.metadata.cpi_handle_mut(),
                mint: accounts.mint.cpi_handle(),
                mint_authority: accounts.mint_authority.cpi_handle(),
                payer: accounts.user.cpi_handle_mut(),
                update_authority: accounts.mint_authority.cpi_handle(),
                system_program: accounts.system_program.cpi_handle(),
                update_authority_is_signer: true,
            },
            signer_seeds,
        ),
        DataV2 {
            name,
            symbol,
            uri,
            seller_fee_basis_points: 0,
            creators: Some(creator),
            collection: None,
            uses: None,
        },
        true,
        Some(CollectionDetails::V1 { size: 0 }),
    )?;
    msg!("Metadata Account created!");

    create_master_edition_v3(
        CpiContext::new_with_signer(
            accounts.token_metadata_program.address(),
            CreateMasterEditionV3 {
                edition: accounts.master_edition.cpi_handle_mut(),
                mint: accounts.mint.cpi_handle_mut(),
                update_authority: accounts.mint_authority.cpi_handle(),
                mint_authority: accounts.mint_authority.cpi_handle(),
                payer: accounts.user.cpi_handle_mut(),
                metadata: accounts.metadata.cpi_handle_mut(),
                token_program: accounts.token_program.cpi_handle(),
                system_program: accounts.system_program.cpi_handle(),
            },
            signer_seeds,
        ),
        Some(0),
    )?;
    msg!("Master Edition Account created");

    Ok(())
}
```

The CPIs go through anchor-spl's `anchor_spl::metadata` wrappers rather than the
`*Cpi` builders in `mpl-token-metadata`. The builders want `&AccountInfo`, and an
account reaches a v2 CPI as a `CpiHandle`, which the wrapper structs take. Each
account slot picks the handle that matches how the CPI uses it: `cpi_handle_mut()`
for the accounts the CPI writes, and `cpi_handle()` for the read-only slots.
`cpi_handle()` takes `&self`, so a single account can fill several read-only slots
in one call, which is what `mint_authority` does here.

Three steps:

1. Mint one token to the destination token account via a CPI to the [Classic Token Program](https://solana.com/docs/terminology#token-program).
2. Create a metadata account for the mint via a CPI to the Token Metadata program. The mint authority signs the CPI, so the `CpiContext` is built with `new_with_signer` and the authority PDA's seeds.
3. Create a master edition account for the mint via a CPI to the Token Metadata program. This enforces the NFT-specific constraints and transfers both the mint authority and freeze authority to the Master Edition PDA. Again, the mint authority signs.

More on Token Metadata: <https://developers.metaplex.com/token-metadata>

## Mint an NFT

The accounts needed to mint an NFT:

```rust
#[derive(Accounts)]
pub struct MintNftAccountConstraints {
    #[account(mut)]
    pub owner: Signer,

    #[account(
        init,
        payer = owner,
        mint::decimals = 0,
        mint::authority = mint_authority,
        mint::freeze_authority = mint_authority,
    )]
    pub mint: Account<Mint>,

    #[account(
        init,
        payer = owner,
        associated_token::mint = mint,
        associated_token::authority = owner
    )]
    pub destination: Account<TokenAccount>,

    #[account(mut)]
    /// CHECK: This account will be initialized by the metaplex program
    pub metadata: UncheckedAccount,

    #[account(mut)]
    /// CHECK: This account will be initialized by the metaplex program
    pub master_edition: UncheckedAccount,

    #[account(
        seeds = [b"authority"],
        bump,
    )]
    /// CHECK: This is account is not initialized and is being used for signing purposes only
    pub mint_authority: UncheckedAccount,

    #[account(mut)]
    pub collection_mint: Account<Mint>,

    pub system_program: Program<System>,
    pub token_program: Program<Token>,
    pub associated_token_program: Program<AssociatedToken>,
    pub token_metadata_program: Program<Metadata>,
}
```

### Account breakdown

- `owner`: the account minting the NFT and the owner of the destination token account.
- `mint`: the NFT mint account. 0 decimals; mint authority and freeze authority are the PDA.
- `destination`: the token account that receives the NFT.
- `metadata`: the metadata account.
- `master_edition`: the master edition account.
- `mint_authority`: the PDA authority used to mint tokens.
- `collection_mint`: the collection the NFT belongs to.
- `system_program`, `token_program`, `associated_token_program`, `token_metadata_program`: as above.

Apart from `collection_mint`, the accounts are the same as the collection creation flow. A collection is just a regular NFT with the `collection_details` field set and the `collection` field on `data` set to `None`. An NFT belonging to a collection has `collection_details` set to `None` and the `collection` field on `data` set to a `Collection` struct with the collection's key and a `verified` boolean. `verified` starts false and flips to true once the NFT is verified as part of the collection.

That's where the `collection_mint` account comes from - it provides the address that goes into the `Collection` struct on the NFT's metadata.

### Implementation for `mint_nft`

`handle_mint_nft` (in `mint_nft.rs`) mirrors `handle_create_collection`: the same caller-supplied `name` / `symbol` / `uri` arguments, the same validation, and the same three CPIs (mint one token, create metadata, create master edition). The difference is in the data on the metadata account.

For the collection NFT:

```rust
create_metadata_accounts_v3(
    cpi_context,
    DataV2 {
        name,
        symbol,
        uri,
        seller_fee_basis_points: 0,
        creators: Some(creator),
        collection: None,
        uses: None,
    },
    true,
    Some(CollectionDetails::V1 { size: 0 }),
)?;
```

We set `collection_details`.

For a regular NFT:

```rust
create_metadata_accounts_v3(
    cpi_context,
    DataV2 {
        name,
        symbol,
        uri,
        seller_fee_basis_points: 0,
        creators: Some(creator),
        collection: Some(Collection {
            verified: false,
            key: *accounts.collection_mint.address(),
        }),
        uses: None,
    },
    true,
    None,
)?;
```

We set the `collection` field with the key of the collection. `verified` starts false until the NFT is verified.

## Verify an NFT as part of a collection

The accounts needed to verify an NFT as part of a collection:

```rust
#[derive(Accounts)]
pub struct VerifyCollectionMintAccountConstraints {
    #[account(mut)]
    pub authority: Signer,
    #[account(mut)]
    pub metadata: MetadataAccount,
    pub mint: Account<Mint>,
    #[account(
        seeds = [b"authority"],
        bump,
    )]
    /// CHECK: This account is not initialized and is being used for signing purposes only
    pub mint_authority: UncheckedAccount,
    pub collection_mint: Account<Mint>,
    #[account(mut)]
    pub collection_metadata: MetadataAccount,
    pub collection_master_edition: MasterEditionAccount,
    pub system_program: Program<System>,
    #[account(address = INSTRUCTIONS_SYSVAR_ID)]
    /// CHECK: Sysvar instruction account that is being checked with an address constraint
    pub sysvar_instruction: UncheckedAccount,
    pub token_metadata_program: Program<Metadata>,
}
```

### Account breakdown

- `authority`: signer of the transaction, and the payer for the verification. You can add constraints to restrict who can verify a collection.
- `metadata`: the metadata account of the NFT being verified.
- `mint`: the NFT mint being verified.
- `mint_authority`: the mint authority of the collection NFT.
- `collection_mint`: the mint account of the collection NFT.
- `collection_metadata`: the metadata account of the collection NFT.
- `collection_master_edition`: the master edition account of the collection NFT.
- `system_program`: as above.
- `sysvar_instruction`: provides access to the serialized instruction data for the running transaction.
- `token_metadata_program`: MPL Token Metadata, used to perform the verification CPI.

The two metadata accounts are mutable because both are updated: the NFT metadata gets its `verified` boolean flipped to true, and the collection NFT metadata has its collection size incremented. `authority` is mutable because it pays for the rent the size increment needs.

### Implementation for `verify_collection`

```rust
pub fn handle_verify_collection(
    accounts: &mut VerifyCollectionMintAccountConstraints,
    bumps: &VerifyCollectionMintAccountConstraintsBumps,
) -> Result<()> {
    let seeds = &[&b"authority"[..], &[bumps.mint_authority]];
    let signer_seeds = &[&seeds[..]];

    verify_sized_collection_item(
        CpiContext::new_with_signer(
            accounts.token_metadata_program.address(),
            VerifySizedCollectionItem {
                metadata: accounts.metadata.cpi_handle_mut(),
                collection_authority: accounts.mint_authority.cpi_handle(),
                payer: accounts.authority.cpi_handle_mut(),
                collection_mint: accounts.collection_mint.cpi_handle(),
                collection_metadata: accounts.collection_metadata.cpi_handle_mut(),
                collection_master_edition: accounts.collection_master_edition.cpi_handle(),
            },
            signer_seeds,
        ),
        None,
    )?;

    msg!("Collection Verified!");

    Ok(())
}
```

The collection was created sized, with `CollectionDetails::V1`, so
`verify_sized_collection_item` is the matching instruction: it is the variant that
also increments the collection's size counter.

> `INSTRUCTIONS_SYSVAR_ID` is the well-known sysvar address `Sysvar1nstructions1111111111111111111111111`, defined directly in [`verify_collection.rs`](programs/mint-nft/src/instructions/verify_collection.rs) because pinocchio, which anchor-lang v2 is built on, does not re-export it.

`verify_collection` performs a CPI to the Token Metadata program with the right accounts. The collection NFT's mint authority signs the CPI, and the NFT is verified as part of the collection.

## Testing

Rust + LiteSVM tests live in `programs/mint-nft/tests/test_nft_operations.rs`. They load the program binary and the Metaplex fixture, then run the full lifecycle - create a collection, mint an NFT into it, verify membership - asserting token balances and that the caller-supplied metadata strings land in the metadata accounts.

```bash
cargo build-sbf
cargo test
```

Use this as a starting point for your own collections, NFTs, and verification flows.
