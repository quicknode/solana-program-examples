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

    // Compute-then-commit: do the fallible add BEFORE mutating either account.
    // The destination is a wallet, so on mainnet this can never overflow (total
    // SOL supply is far below u64::MAX), but ordering the check first means an
    // overflow returns Err with no state changed - conservation holds on every
    // path from the function's own logic, not just because the runtime reverts a
    // failed instruction. Zeroing the source before the fallible credit would
    // transiently destroy lamports on the error path.
    let new_destination_lamports = destination_lamports
        .checked_add(offer_lamports)
        .ok_or(EscrowError::ArithmeticOverflow)?;

    **destination.lamports.borrow_mut() = new_destination_lamports;
    **offer_info.lamports.borrow_mut() = 0;

    offer_info.resize(0)?;
    offer_info.assign(system_program.key);

    Ok(())
}
