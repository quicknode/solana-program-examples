// On-chain prize program for a hackathon run by a Squads multisig committee.
//
// The program does not implement multisig logic. It treats the
// `Hackathon.authority` as an opaque "admin" pubkey and only checks
// `signer == hackathon.authority` on privileged instruction handlers. In
// practice the authority is a Squads vault PDA: Squads handles propose/vote
// /execute off-program, and when execution lands the program just sees a
// signed CPI from the vault.
//
// This keeps the program small and lets the committee swap multisig
// implementations without touching the prize program.

pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use instructions::*;
pub use state::*;

declare_id!("71AxoNytgqQrSFMvGREPeJ1E2btEoTMw8J4FALsmNcGx");

#[program]
pub mod hackathon {
    use super::*;

    // Create a hackathon controlled by `authority` (in practice a Squads vault
    // PDA). The hackathon's name is hashed into the PDA seeds so the same
    // authority can run multiple hackathons.
    pub fn create_hackathon(context: Context<CreateHackathon>, name: String) -> Result<()> {
        instructions::create_hackathon::handle_create_hackathon(context, name)
    }

    // Register a new prize under an existing hackathon. The mint and target
    // amount are recorded on the Prize account; the vault ATA is created here
    // with the Prize PDA as its authority. Must be signed by the hackathon
    // authority.
    pub fn add_prize(context: Context<AddPrize>, amount: u64) -> Result<()> {
        instructions::add_prize::handle_add_prize(context, amount)
    }

    // Record the winner for a prize. Must be signed by the hackathon
    // authority. Errors if the prize has already been paid or cancelled.
    pub fn set_winner(context: Context<SetWinner>, prize_index: u8, winner: Pubkey) -> Result<()> {
        instructions::set_winner::handle_set_winner(context, prize_index, winner)
    }

    // Pay the recorded winner the exact `prize.amount`. Unpermissioned: any
    // signer can trigger payment once the winner is set and the vault is
    // funded. Any surplus left in the vault stays there and can be reclaimed
    // by the authority via `cancel_prize`.
    pub fn pay_winner(context: Context<PayWinner>, prize_index: u8) -> Result<()> {
        instructions::pay_winner::handle_pay_winner(context, prize_index)
    }

    // Cancel a prize that has not yet been paid: drains the vault to
    // `refund_to`, closes the vault, and marks the prize cancelled so it can
    // no longer be paid. Must be signed by the hackathon authority.
    pub fn cancel_prize(context: Context<CancelPrize>, prize_index: u8) -> Result<()> {
        instructions::cancel_prize::handle_cancel_prize(context, prize_index)
    }

    // Close the hackathon and refund its rent to the authority. Only allowed
    // once every registered prize is either paid or cancelled. Must be signed
    // by the hackathon authority.
    //
    // Individual Prize accounts are not closed here. They remain on-chain as
    // an immutable record of who won what; closing them would erase that
    // history for a small rent refund.
    pub fn close_hackathon(context: Context<CloseHackathon>) -> Result<()> {
        instructions::close_hackathon::handle_close_hackathon(context)
    }
}
