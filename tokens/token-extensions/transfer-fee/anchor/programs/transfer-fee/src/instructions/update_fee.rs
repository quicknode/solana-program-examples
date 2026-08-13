use anchor_lang::prelude::*;
use anchor_spl::token_interface::{transfer_fee_set, Mint, Token2022, TransferFeeSetTransferFee};

#[derive(Accounts)]
pub struct UpdateFeeAccountConstraints {
    pub authority: Signer,

    #[account(mut)]
    pub mint_account: InterfaceAccount<Mint>,
    pub token_program: Program<Token2022>,
}

// Note that there is a 2 epoch delay from when new fee updates take effect
// This is a safely feature built into the extension
// https://github.com/solana-program/token-2022/blob/2d18d97f083627d3f13ce43b16fa4305cbfac4de/program/src/extension/transfer_fee/processor.rs#L92-L109
pub fn handle_process_update_fee(
    context: &mut Context<UpdateFeeAccountConstraints>,
    transfer_fee_basis_points: u16,
    maximum_fee: u64,
) -> Result<()> {
    transfer_fee_set(
        CpiContext::new(
            context.accounts.token_program.address(),
            TransferFeeSetTransferFee {
                mint: context.accounts.mint_account.cpi_handle_mut(),
                authority: context.accounts.authority.cpi_handle(),
            },
        ),
        transfer_fee_basis_points, // transfer fee basis points (% fee per transfer)
        maximum_fee,               // maximum fee (maximum units of token per transfer)
    )?;
    Ok(())
}
