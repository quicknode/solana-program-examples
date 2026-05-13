# Using Token Account Data as a Seed in a Transfer Hook

Sometimes you want to use account data to derive additional accounts in the extra-account-metas. For example, you might want to use the token account's owner as a seed for a PDA.

When creating an `ExtraAccountMeta`, the data of any account can be used as an extra seed. In this example we derive a counter account from the token account owner and the literal `"counter"`. The counter records how many times that owner has transferred tokens.

This is the setup in `extra_account_metas()`:

```rust
// Define extra account metas to store on the extra_account_meta_list account
impl<'info> InitializeExtraAccountMetaList<'info> {
    pub fn extra_account_metas() -> Result<Vec<ExtraAccountMeta>> {
        Ok(vec![ExtraAccountMeta::new_with_seeds(
            &[
                Seed::Literal { bytes: b"counter".to_vec() },
                Seed::AccountData {
                    account_index: 0,
                    data_index: 32,
                    length: 32,
                },
            ],
            false, // is_signer
            true,  // is_writable
        )?])
    }
}
```

The token account layout is what makes `data_index: 32, length: 32` mean "the owner field". Bytes 0..32 are the mint and bytes 32..64 are the owner:

```rust
/// Token account data.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Account {
    pub mint: Pubkey,
    pub owner: Pubkey,
    pub amount: u64,
    pub delegate: COption<Pubkey>,
    pub state: AccountState,
    pub is_native: COption<u64>,
    pub delegated_amount: u64,
    pub close_authority: COption<Pubkey>,
}
```

`account_index: 0` means the source token account, which is always the first account in a transfer hook's accounts array. The second is always the mint; the third is always the destination token account. The order matches the legacy token program.

Because we derive the counter account from the *sender's* token account owner, we `init` the counter PDA when we initialize the `ExtraAccountMeta` list. Once initialized, the transfer hook increments the counter on every transfer:

```rust
#[derive(Accounts)]
pub struct InitializeExtraAccountMetaList<'info> {
    #[account(mut)]
    payer: Signer<'info>,

    /// CHECK: ExtraAccountMetaList account, must use these seeds.
    #[account(
        init,
        seeds = [b"extra-account-metas", mint.key().as_ref()],
        bump,
        space = ExtraAccountMetaList::size_of(
            InitializeExtraAccountMetaList::extra_account_metas()?.len()
        )?,
        payer = payer,
    )]
    pub extra_account_meta_list: AccountInfo<'info>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(
        init,
        seeds = [b"counter", payer.key().as_ref()],
        bump,
        payer = payer,
        space = COUNTER_ACCOUNT_SIZE,
    )]
    pub counter_account: Account<'info, CounterAccount>,
    pub token_program: Program<'info, Token2022>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}
```

The counter account also has to appear on the `TransferHook` struct — the program needs to know about every account passed in by the runtime:

```rust
#[derive(Accounts)]
pub struct TransferHook<'info> {
    #[account(token::mint = mint, token::authority = owner)]
    pub source_token: InterfaceAccount<'info, TokenAccount>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(token::mint = mint)]
    pub destination_token: InterfaceAccount<'info, TokenAccount>,
    /// CHECK: source token account owner; may be a SystemAccount or a PDA owned by another program.
    pub owner: UncheckedAccount<'info>,
    /// CHECK: ExtraAccountMetaList account.
    #[account(seeds = [b"extra-account-metas", mint.key().as_ref()], bump)]
    pub extra_account_meta_list: UncheckedAccount<'info>,
    #[account(seeds = [b"counter", owner.key().as_ref()], bump)]
    pub counter_account: Account<'info, CounterAccount>,
}
```

On the client side, the helper resolves the extra account for you:

```typescript
const transferInstructionWithHelper = await createTransferCheckedWithTransferHookInstruction(
    connection,
    sourceTokenAccount,
    mint.publicKey,
    destinationTokenAccount,
    wallet.publicKey,
    amountBigInt,
    decimals,
    [],
    "confirmed",
    TOKEN_2022_PROGRAM_ID,
);
```

If you wanted to derive the counter PDA manually:

```typescript
const [counterPDA] = PublicKey.findProgramAddressSync(
    [Buffer.from("counter"), wallet.publicKey.toBuffer()],
    program.programId,
);
```

Note: the counter account must exist before a transfer, since the hook reads/writes it. In this example we initialize it alongside the extra-account-metas, so there's only ever one counter — the one for the wallet that initialized the metas. If you want a counter per holder, you'd need to expose an opt-in handler to create it (a "sign up for counter" button in your dapp, for example).
