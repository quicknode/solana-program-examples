use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct Prize {
    // Parent hackathon. Kept here so a Prize can be loaded and validated
    // without first loading the Hackathon.
    pub hackathon: Pubkey,
    // Stable index assigned at creation time (= `hackathon.prize_count` at
    // the moment `add_prize` ran). Used in the Prize PDA seeds, so it never
    // changes.
    pub index: u8,
    // The token mint this prize is denominated in. One mint per prize, so a
    // single hackathon can mix denominations (e.g. USDC for cash prizes,
    // governance token for runner-up prizes).
    pub mint: Pubkey,
    // Exact amount paid to the winner. `pay_winner` always transfers this
    // amount; surplus in the vault remains until reclaimed via cancel_prize.
    pub amount: u64,
    // Recorded winner. `None` until `set_winner` runs.
    pub winner: Option<Pubkey>,
    pub paid: bool,
    pub cancelled: bool,
    pub bump: u8,
}
