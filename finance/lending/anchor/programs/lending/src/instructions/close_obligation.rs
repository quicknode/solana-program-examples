use anchor_lang::prelude::*;

use crate::errors::LendingError;
use crate::state::Obligation;

/// Close an obligation that holds nothing, returning its rent to the owner.
///
/// The obligation must have no deposited collateral and no borrows: a deposit
/// entry is removed when its last share is withdrawn and a borrow entry when
/// its last unit is repaid, so both lists being empty means the position is
/// fully unwound. Closing one that still holds either would strand the
/// collateral in its vault, or forgive the debt, so the handler refuses with
/// `ObligationNotEmpty`. Only the owner may close it (`address =
/// obligation.owner`), since the rent is theirs and a stranger could otherwise
/// close a position its owner means to use again. The account itself closes
/// through Anchor's `close = owner` constraint once the handler returns, which
/// hands the owner every lamport it holds: its own rent, plus the rent of any
/// collateral vault a liquidation emptied and closed into it.
pub fn handle_close_obligation(context: &mut Context<CloseObligation>) -> Result<()> {
    let obligation = &context.accounts.obligation;
    require!(
        obligation.deposits.is_empty() && obligation.borrows.is_empty(),
        LendingError::ObligationNotEmpty
    );
    Ok(())
}

#[derive(Accounts)]
pub struct CloseObligation {
    #[account(mut, close = owner)]
    pub obligation: BorshAccount<Obligation>,

    #[account(mut, address = obligation.owner)]
    pub owner: Signer,
}
