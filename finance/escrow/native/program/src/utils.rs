use crate::error::EscrowError;
use solana_program::{account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey};

pub fn assert_is_associated_token_account(
    token_address: &Pubkey,
    owner: &Pubkey,
    mint: &Pubkey,
) -> Result<(), ProgramError> {
    let associated_token_account_address =
        &spl_associated_token_account_interface::address::get_associated_token_address(owner, mint);

    if token_address != associated_token_account_address {
        return Err(EscrowError::TokenAccountMismatch.into());
    }

    Ok(())
}

// Close a program-owned account: move all of its lamports to `destination`
// (the party who paid its rent), wipe its data, and hand it back to the
// System Program.
pub fn close_offer_account<'info>(
    offer_info: &AccountInfo<'info>,
    destination: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
) -> Result<(), ProgramError> {
    let offer_lamports = offer_info.lamports();
    let destination_lamports = destination.lamports();

    **offer_info.lamports.borrow_mut() = 0;
    **destination.lamports.borrow_mut() = destination_lamports
        .checked_add(offer_lamports)
        .ok_or(EscrowError::ArithmeticOverflow)?;

    offer_info.resize(0)?;
    offer_info.assign(system_program.key);

    Ok(())
}
