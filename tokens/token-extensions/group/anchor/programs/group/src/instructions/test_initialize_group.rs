use anchor_lang::prelude::*;
use anchor_lang::system_program::{create_account, CreateAccount};
use anchor_spl::{
    token_2022::{
        initialize_mint2,
        spl_token_2022::{
            extension::{group_pointer::GroupPointer, ExtensionType},
            pod::PodMint,
        },
        InitializeMint2,
    },
    token_2022_extensions::group_pointer::{group_pointer_initialize, GroupPointerInitialize},
    token_interface::{
        spl_token_2022::{
            extension::{BaseStateWithExtensions, StateWithExtensions},
            state::Mint as MintState,
        },
        Token2022,
    },
};

#[derive(Accounts)]
pub struct InitializeGroupAccountConstraints {
    #[account(mut)]
    pub payer: Signer,

    /// CHECK: created and initialized by this instruction as a Token-2022 mint
    /// carrying the GroupPointer extension.
    #[account(
        mut,
        seeds = [b"group"],
        bump,
    )]
    pub mint_account: UncheckedAccount,
    pub token_program: Program<Token2022>,
    pub system_program: Program<System>,
}

fn check_mint_data(context: &Context<InitializeGroupAccountConstraints>) -> Result<()> {
    let mint = context.accounts.mint_account.account();
    let mint_data = mint.try_borrow()?;
    let mint_with_extension = StateWithExtensions::<MintState>::unpack(&mint_data)?;
    let extension_data = mint_with_extension.get_extension::<GroupPointer>()?;

    msg!("{:?}", extension_data);
    Ok(())
}

// There is currently not an anchor constraint to automatically initialize the
// GroupPointer extension. We can manually create and initialize the mint
// account via CPIs in the instruction handler. The mint is a PDA, so it signs
// its own creation with its seeds.
pub fn handler(context: &mut Context<InitializeGroupAccountConstraints>) -> Result<()> {
    let bump = context.bumps.mint_account;
    let signer_seeds: &[&[&[u8]]] = &[&[b"group", &[bump]]];

    // Calculate space required for mint and extension data
    let mint_size =
        ExtensionType::try_calculate_account_len::<PodMint>(&[ExtensionType::GroupPointer])?;

    // Calculate minimum lamports required for size of mint account with extensions
    let lamports = Rent::get()?.try_minimum_balance(mint_size)?;

    // The mint PDA is both the authority and the group address, so take a copy
    // of its `AccountView` for the read-only uses — v2's typed handles enforce
    // borrow exclusivity at compile time.
    let mint_address = *context.accounts.mint_account.address();

    // Invoke System Program to create new account with space for mint and extension data
    create_account(
        CpiContext::new(
            context.accounts.system_program.address(),
            CreateAccount {
                from: context.accounts.payer.cpi_handle_mut(),
                to: context.accounts.mint_account.cpi_handle_mut(),
            },
        )
        .with_signer(signer_seeds),
        lamports,                                 // Lamports
        mint_size as u64,                         // Space
        context.accounts.token_program.address(), // Owner Program
    )?;

    // Initialize the GroupPointer extension
    // This instruction must come before the instruction to initialize the mint data
    group_pointer_initialize(
        CpiContext::new(
            context.accounts.token_program.address(),
            GroupPointerInitialize {
                mint: context.accounts.mint_account.cpi_handle_mut(),
            },
        ),
        Some(&mint_address),
        Some(&mint_address),
    )?;

    // Initialize the standard mint account data
    initialize_mint2(
        CpiContext::new(
            context.accounts.token_program.address(),
            InitializeMint2 {
                mint: context.accounts.mint_account.cpi_handle_mut(),
            },
        ),
        2,                   // decimals
        &mint_address,       // mint authority
        Some(&mint_address), // freeze authority
    )?;

    check_mint_data(context)?;

    // Token Group and Token Member extensions features not enabled yet on the Token2022 program
    // This is temporary placeholder to update once extensions are live
    // Initializing the "pointers" works, but you can't initialize the group/member data yet
    Ok(())
}
