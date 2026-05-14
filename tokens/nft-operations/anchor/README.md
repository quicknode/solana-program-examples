# NFT Operations

Create an NFT collection, mint an NFT, and verify an NFT as part of a collection — all using Metaplex Token Metadata.

## Program setup

This example clones the Metaplex Token Metadata [program](https://solana.com/docs/terminology#program) from mainnet. See `Anchor.toml`:

```toml
[test.validator]
url = "https://api.mainnet-beta.solana.com"

[[test.validator.clone]]
address = "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
```

The program is needed for [CPIs](https://solana.com/docs/terminology#cross-program-invocation-cpi) that create metadata [accounts](https://solana.com/docs/terminology#account) and master edition accounts, and to verify NFTs as part of a collection.

## Create an NFT collection

The accounts needed to create an NFT collection are:

```rust
#[derive(Accounts)]
pub struct CreateCollection<'info> {
    #[account(mut)]
    user: Signer<'info>,
    #[account(
        init,
        payer = user,
        mint::decimals = 0,
        mint::authority = mint_authority,
        mint::freeze_authority = mint_authority,
    )]
    mint: Account<'info, Mint>,
    #[account(
        seeds = [b"authority"],
        bump,
    )]
    /// CHECK: This account is not initialized and is being used for signing purposes only
    pub mint_authority: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: This account will be initialized by the metaplex program
    metadata: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: This account will be initialized by the metaplex program
    master_edition: UncheckedAccount<'info>,
    #[account(
        init,
        payer = user,
        associated_token::mint = mint,
        associated_token::authority = user
    )]
    destination: Account<'info, TokenAccount>,
    system_program: Program<'info, System>,
    token_program: Program<'info, Token>,
    associated_token_program: Program<'info, AssociatedToken>,
    token_metadata_program: Program<'info, Metadata>,
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

Both `metadata` and `master_edition` are `UncheckedAccount` because they are uninitialized at the start of the [instruction](https://solana.com/docs/terminology#instruction) — the Token Metadata program initializes them via CPI.

Had we written:

```rust
#[derive(Accounts)]
pub struct CreateCollection<'info> {
    #[account(mut)]
    metadata: Account<'info, MetadataAccount>,
    #[account(mut)]
    master_edition: Account<'info, MasterEditionAccount>,
}
```

the instruction would fail because [Anchor](https://solana.com/docs/terminology#anchor) would expect the accounts to already be initialized.

When an account *is* already initialized (as in the verify-collection flow below), use the specific account types.

### Implementation for `CreateCollection`

Each [instruction handler](https://solana.com/docs/terminology#instruction-handler) is a free function (`pub fn handler(accounts: &mut X, bumps: &XBumps)`) called from the `#[program]` module in `lib.rs`. The account-validation struct lives in the same file as the handler.

```rust
pub fn handler(accounts: &mut CreateCollection, bumps: &CreateCollectionBumps) -> Result<()> {

    let metadata = &accounts.metadata.to_account_info();
    let master_edition = &accounts.master_edition.to_account_info();
    let mint = &accounts.mint.to_account_info();
    let authority = &accounts.mint_authority.to_account_info();
    let payer = &accounts.user.to_account_info();
    let system_program = &accounts.system_program.to_account_info();
    let spl_token_program = &accounts.token_program.to_account_info();
    let spl_metadata_program = &accounts.token_metadata_program.to_account_info();

    let seeds = &[&b"authority"[..], &[bumps.mint_authority]];
    let signer_seeds = &[&seeds[..]];

    let cpi_accounts = MintTo {
        mint: accounts.mint.to_account_info(),
        to: accounts.destination.to_account_info(),
        authority: accounts.mint_authority.to_account_info(),
    };
    let cpi_ctx = CpiContext::new_with_signer(accounts.token_program.key(), cpi_accounts, signer_seeds);
    mint_to(cpi_ctx, 1)?;
    msg!("Collection NFT minted!");

    let creator = vec![Creator {
        address: accounts.mint_authority.key().clone(),
        verified: true,
        share: 100,
    }];

    let metadata_account = CreateMetadataAccountV3Cpi::new(
        spl_metadata_program,
        CreateMetadataAccountV3CpiAccounts {
            metadata, mint, mint_authority: authority, payer,
            update_authority: (authority, true),
            system_program,
            rent: None,
        },
        CreateMetadataAccountV3InstructionArgs {
            data: DataV2 {
                name: "DummyCollection".to_owned(),
                symbol: "DC".to_owned(),
                uri: "".to_owned(),
                seller_fee_basis_points: 0,
                creators: Some(creator),
                collection: None,
                uses: None,
            },
            is_mutable: true,
            collection_details: Some(CollectionDetails::V1 { size: 0 }),
        },
    );
    metadata_account.invoke_signed(signer_seeds)?;
    msg!("Metadata Account created!");

    let master_edition_account = CreateMasterEditionV3Cpi::new(
        spl_metadata_program,
        CreateMasterEditionV3CpiAccounts {
            edition: master_edition,
            update_authority: authority,
            mint_authority: authority,
            mint, payer, metadata,
            token_program: spl_token_program,
            system_program,
            rent: None,
        },
        CreateMasterEditionV3InstructionArgs { max_supply: Some(0) },
    );
    master_edition_account.invoke_signed(signer_seeds)?;
    msg!("Master Edition Account created");

    Ok(())
}
```

Three steps:

1. Mint one token to the destination token account via a CPI to the [Classic Token Program](https://solana.com/docs/terminology#token-program).
2. Create a metadata account for the mint via a CPI to the Token Metadata program. The mint authority signs the CPI, so we use `invoke_signed` with the authority PDA's seeds.
3. Create a master edition account for the mint via a CPI to the Token Metadata program. This enforces the NFT-specific constraints and transfers both the mint authority and freeze authority to the Master Edition PDA. Again, the mint authority signs.

More on Token Metadata: <https://developers.metaplex.com/token-metadata>

## Mint an NFT

The accounts needed to mint an NFT:

```rust
#[derive(Accounts)]
pub struct MintNFT<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(
        init,
        payer = owner,
        mint::decimals = 0,
        mint::authority = mint_authority,
        mint::freeze_authority = mint_authority,
    )]
    pub mint: Account<'info, Mint>,
    #[account(
        init,
        payer = owner,
        associated_token::mint = mint,
        associated_token::authority = owner
    )]
    pub destination: Account<'info, TokenAccount>,
    #[account(mut)]
    /// CHECK: This account will be initialized by the metaplex program
    pub metadata: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: This account will be initialized by the metaplex program
    pub master_edition: UncheckedAccount<'info>,
    #[account(
        seeds = [b"authority"],
        bump,
    )]
    /// CHECK: This is account is not initialized and is being used for signing purposes only
    pub mint_authority: UncheckedAccount<'info>,
    #[account(mut)]
    pub collection_mint: Account<'info, Mint>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_metadata_program: Program<'info, Metadata>,
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

That's where the `collection` account comes from — it provides the address that goes into the `Collection` struct on the NFT's metadata.

### Implementation for `MintNFT`

```rust
pub fn handler(accounts: &mut MintNFT, bumps: &MintNFTBumps) -> Result<()> {

    let metadata = &accounts.metadata.to_account_info();
    let master_edition = &accounts.master_edition.to_account_info();
    let mint = &accounts.mint.to_account_info();
    let authority = &accounts.mint_authority.to_account_info();
    let payer = &accounts.owner.to_account_info();
    let system_program = &accounts.system_program.to_account_info();
    let spl_token_program = &accounts.token_program.to_account_info();
    let spl_metadata_program = &accounts.token_metadata_program.to_account_info();

    let seeds = &[&b"authority"[..], &[bumps.mint_authority]];
    let signer_seeds = &[&seeds[..]];

    let cpi_accounts = MintTo {
        mint: accounts.mint.to_account_info(),
        to: accounts.destination.to_account_info(),
        authority: accounts.mint_authority.to_account_info(),
    };
    let cpi_ctx = CpiContext::new_with_signer(accounts.token_program.key(), cpi_accounts, signer_seeds);
    mint_to(cpi_ctx, 1)?;
    msg!("Collection NFT minted!");

    let creator = vec![Creator {
        address: accounts.mint_authority.key(),
        verified: true,
        share: 100,
    }];

    let metadata_account = CreateMetadataAccountV3Cpi::new(
        spl_metadata_program,
        CreateMetadataAccountV3CpiAccounts {
            metadata, mint, mint_authority: authority, payer,
            update_authority: (authority, true),
            system_program,
            rent: None,
        },
        CreateMetadataAccountV3InstructionArgs {
            data: DataV2 {
                name: "Mint Test".to_string(),
                symbol: "YAY".to_string(),
                uri: "".to_string(),
                seller_fee_basis_points: 0,
                creators: Some(creator),
                collection: Some(Collection {
                    verified: false,
                    key: accounts.collection_mint.key(),
                }),
                uses: None,
            },
            is_mutable: true,
            collection_details: None,
        },
    );
    metadata_account.invoke_signed(signer_seeds)?;

    let master_edition_account = CreateMasterEditionV3Cpi::new(
        spl_metadata_program,
        CreateMasterEditionV3CpiAccounts {
            edition: master_edition,
            update_authority: authority,
            mint_authority: authority,
            mint, payer, metadata,
            token_program: spl_token_program,
            system_program,
            rent: None,
        },
        CreateMasterEditionV3InstructionArgs { max_supply: Some(0) },
    );
    master_edition_account.invoke_signed(signer_seeds)?;

    Ok(())
}
```

Because a collection NFT is just a regular NFT with special metadata, the implementation mirrors `CreateCollection`. The same three steps:

1. Mint one token to the destination via a Classic Token Program CPI.
2. Create a metadata account via a Token Metadata CPI (signed with the PDA seeds).
3. Create a master edition account via a Token Metadata CPI (signed with the PDA seeds).

The difference is in the data on the metadata account.

For the collection NFT:
```rust
CreateMetadataAccountV3InstructionArgs {
    data: DataV2 {
        name: "DummyCollection".to_owned(),
        symbol: "DC".to_owned(),
        uri: "".to_owned(),
        seller_fee_basis_points: 0,
        creators: Some(creator),
        collection: None,
        uses: None,
    },
    is_mutable: true,
    collection_details: Some(
        CollectionDetails::V1 { 
            size: 0 
        }
    )
}
```
We set `collection_details`.

For a regular NFT:
```rust
CreateMetadataAccountV3InstructionArgs {
    data: DataV2 {
        name: "Mint Test".to_string(),
        symbol: "YAY".to_string(),
        uri: "".to_string(),
        seller_fee_basis_points: 0,
        creators: Some(creator),
        collection: Some(Collection {
            verified: false,
            key: self.collection_mint.key(),
        }),
        uses: None
    },
    is_mutable: true,
    collection_details: None,
}
```
We set the `collection` field with the key of the collection. `verified` starts false until the NFT is verified.

## Verify an NFT as part of a collection

The accounts needed to verify an NFT as part of a collection:

```rust
#[derive(Accounts)]
pub struct VerifyCollectionMint<'info> {
    pub authority: Signer<'info>,
    #[account(mut)]
    pub metadata: Account<'info, MetadataAccount>,
    pub mint: Account<'info, Mint>,
    #[account(
        seeds = [b"authority"],
        bump,
    )]
    /// CHECK: This account is not initialized and is being used for signing purposes only
    pub mint_authority: UncheckedAccount<'info>,
    pub collection_mint: Account<'info, Mint>,
    #[account(mut)]
    pub collection_metadata: Account<'info, MetadataAccount>,
    pub collection_master_edition: Account<'info, MasterEditionAccount>,
    pub system_program: Program<'info, System>,
    #[account(address = INSTRUCTIONS_SYSVAR_ID)]
    /// CHECK: Sysvar instruction account that is being checked with an address constraint
    pub sysvar_instruction: UncheckedAccount<'info>,
    pub token_metadata_program: Program<'info, Metadata>,
}
```

### Account breakdown

- `authority`: signer of the transaction. You can add constraints to restrict who can verify a collection.
- `metadata`: the metadata account of the NFT being verified.
- `mint`: the NFT mint being verified.
- `mint_authority`: the mint authority of the collection NFT.
- `collection_mint`: the mint account of the collection NFT.
- `collection_metadata`: the metadata account of the collection NFT.
- `collection_master_edition`: the master edition account of the collection NFT.
- `system_program`: as above.
- `sysvar_instruction`: provides access to the serialized instruction data for the running transaction.
- `token_metadata_program`: MPL Token Metadata, used to perform the verification CPI.

Only the NFT and collection NFT metadata accounts need to be mutable — both are updated. The NFT metadata gets its `verified` boolean flipped to true, and the collection NFT metadata has its collection size incremented.

### Implementation for `VerifyCollectionMint`

```rust
pub fn handler(accounts: &mut VerifyCollectionMint, bumps: &VerifyCollectionMintBumps) -> Result<()> {
    let metadata = &accounts.metadata.to_account_info();
    let authority = &accounts.mint_authority.to_account_info();
    let collection_mint = &accounts.collection_mint.to_account_info();
    let collection_metadata = &accounts.collection_metadata.to_account_info();
    let collection_master_edition = &accounts.collection_master_edition.to_account_info();
    let system_program = &accounts.system_program.to_account_info();
    let sysvar_instructions = &accounts.sysvar_instruction.to_account_info();
    let spl_metadata_program = &accounts.token_metadata_program.to_account_info();

    let seeds = &[&b"authority"[..], &[bumps.mint_authority]];
    let signer_seeds = &[&seeds[..]];

    let verify_collection = VerifyCollectionV1Cpi::new(
        spl_metadata_program,
        VerifyCollectionV1CpiAccounts {
            authority,
            delegate_record: None,
            metadata,
            collection_mint,
            collection_metadata: Some(collection_metadata),
            collection_master_edition: Some(collection_master_edition),
            system_program,
            sysvar_instructions,
        },
    );
    verify_collection.invoke_signed(signer_seeds)?;

    msg!("Collection Verified!");
    Ok(())
}
```

> `INSTRUCTIONS_SYSVAR_ID` is the well-known sysvar address `Sysvar1nstructions1111111111111111111111111`, defined directly in [`verify_collection.rs`](programs/mint-nft/src/instructions/verify_collection.rs) because `sysvar::instructions::ID` moved in Anchor 1.0.

`verify_collection` performs a CPI to the Token Metadata program with the right accounts. The collection NFT's mint authority signs the CPI, and the NFT is verified as part of the collection.

Use this as a starting point for your own collections, NFTs, and verification flows.
