use anchor_lang::prelude::*;

use crate::error::HackathonError;
use crate::state::Hackathon;

// Close the Hackathon account and refund its rent to `rent_destination`.
// Permitted only once every registered Prize is either paid or cancelled —
// the handler reads each Prize account from `remaining_accounts` and checks
// its state. This avoids storing a separate `active_prize_count` field that
// could drift out of sync with the per-Prize flags.
#[derive(Accounts)]
pub struct CloseHackathon<'info> {
    pub authority: Signer<'info>,

    #[account(mut)]
    pub rent_destination: SystemAccount<'info>,

    #[account(
        mut,
        has_one = authority,
        close = rent_destination,
        seeds = [b"hackathon", authority.key().as_ref(), super::name_seed(&hackathon.name).as_ref()],
        bump = hackathon.bump,
    )]
    pub hackathon: Account<'info, Hackathon>,
}

pub fn handle_close_hackathon(context: Context<CloseHackathon>) -> Result<()> {
    let hackathon = &context.accounts.hackathon;

    // Caller must pass every Prize account for this hackathon as remaining
    // accounts (in index order). Each one must either be paid or cancelled.
    require!(
        context.remaining_accounts.len() == hackathon.prize_count as usize,
        HackathonError::PrizesStillActive
    );

    for (expected_index, prize_account_info) in context.remaining_accounts.iter().enumerate() {
        let prize = Account::<crate::state::Prize>::try_from(prize_account_info)?;
        require_keys_eq!(
            prize.hackathon,
            hackathon.key(),
            HackathonError::PrizesStillActive
        );
        require!(
            prize.index == expected_index as u8,
            HackathonError::PrizesStillActive
        );
        require!(
            prize.paid || prize.cancelled,
            HackathonError::PrizesStillActive
        );
    }

    Ok(())
}
