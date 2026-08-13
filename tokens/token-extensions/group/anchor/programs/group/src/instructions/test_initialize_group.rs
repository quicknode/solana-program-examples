use anchor_lang::prelude::*;
use anchor_spl::mint;
use anchor_spl::token_2022::spl_token_2022::extension::group_pointer::GroupPointer;
use anchor_spl::token_interface::{
    spl_token_2022::{
        extension::{BaseStateWithExtensions, StateWithExtensions},
        state::Mint as MintState,
    },
    Mint, Token2022,
};

#[derive(Accounts)]
pub struct InitializeGroupAccountConstraints {
    #[account(mut)]
    pub payer: Signer,

    #[account(
        init,
        seeds = [b"group"],
        bump,
        payer = payer,
        mint::decimals = 2,
        mint::authority = mint_account,
        mint::freeze_authority = mint_account,
        extensions::group_pointer::authority = mint_account,
        extensions::group_pointer::group_address = mint_account,
    )]
    pub mint_account: InterfaceAccount<Mint>,
    pub token_program: Program<Token2022>,
    pub system_program: Program<System>,
}

fn check_mint_data(accounts: &mut InitializeGroupAccountConstraints) -> Result<()> {
    let mint = &accounts.mint_account.cpi_handle_mut();
    let mint_data = mint.data.borrow();
    let mint_with_extension = StateWithExtensions::<MintState>::unpack(&mint_data)?;
    let extension_data = mint_with_extension.get_extension::<GroupPointer>()?;

    msg!("{:?}", mint_with_extension);
    msg!("{:?}", extension_data);
    Ok(())
}

pub fn handler(mut context: &mut Context<InitializeGroupAccountConstraints>) -> Result<()> {
    check_mint_data(&mut context.accounts)?;

    // // Token Group and Token Member extensions features not enabled yet on the Token2022 program
    // // This is temporary placeholder to update one extensions are live
    // // Initializing the "pointers" works, but you can't initialize the group/member data yet

    // let signer_seeds: &[&[&[u8]]] = &[&[b"group", &[context.bumps.mint_account]]];
    // token_group_initialize(
    //     CpiContext::new(
    //         context.accounts.token_program.cpi_handle_mut(),
    //         TokenGroupInitialize {
    //             token_program_id: context.accounts.token_program.cpi_handle_mut(),
    //             group: context.accounts.mint_account.cpi_handle_mut(),
    //             mint: context.accounts.mint_account.cpi_handle_mut(),
    //             mint_authority: context.accounts.mint_account.cpi_handle_mut(),
    //         },
    //     )
    //     .with_signer(signer_seeds),
    //     Some(context.accounts.payer.address()), // update_authority
    //     10,                             // max_size
    // )?;
    Ok(())
}
