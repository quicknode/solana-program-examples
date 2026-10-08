pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use instructions::*;
pub use state::*;

declare_id!("qbuMdeYxYJXBjU6C6qFKjZKjXmrU83eDQomHdrch826");

#[program]
pub mod escrow {
    use super::*;

    pub fn make_offer(
        context: &mut Context<MakeOfferAccountConstraints>,
        id: u64,
        token_a_offered_amount: u64,
        token_b_wanted_amount: u64,
    ) -> Result<()> {
        instructions::make_offer::handle_validate_offer(
            token_a_offered_amount,
            token_b_wanted_amount,
        )?;
        instructions::make_offer::handle_send_offered_tokens_to_vault(
            context,
            token_a_offered_amount,
        )?;
        instructions::make_offer::handle_save_offer(context, id, token_b_wanted_amount)
    }

    // The taker signs the terms they agreed to. An offer's address is its
    // maker and id, so a maker can cancel and re-make the same id at worse
    // terms while the taker's transaction is in flight; these bounds make that
    // transaction fail instead of trading at the new terms.
    pub fn take_offer(
        context: &mut Context<TakeOfferAccountConstraints>,
        minimum_token_a_out: u64,
        maximum_token_b_in: u64,
    ) -> Result<()> {
        instructions::take_offer::handle_check_offer_terms(
            context,
            minimum_token_a_out,
            maximum_token_b_in,
        )?;
        instructions::take_offer::handle_send_wanted_tokens_to_maker(context)?;
        instructions::take_offer::handle_withdraw_and_close_vault(context)
    }

    // Cancel an outstanding offer. The maker signs, the vault tokens flow back
    // to the maker, and both the vault and offer accounts are closed (rent
    // refunded to the maker). Without this, abandoned offers would lock funds
    // forever.
    pub fn cancel_offer(context: &mut Context<CancelOfferAccountConstraints>) -> Result<()> {
        instructions::cancel_offer::handle_cancel_offer(context)
    }
}
