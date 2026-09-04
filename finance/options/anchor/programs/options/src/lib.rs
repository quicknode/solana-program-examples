use anchor_lang::prelude::*;

mod constants;
mod errors;
// Public so the LiteSVM integration tests can build instruction arguments
// (`OptionTerms`, `OptionKind`) against the program's own types, and the
// proofs crate's README can point at the formulas it mirrors.
pub mod contract_math;
pub mod instructions;
pub mod state;

use instructions::*;

declare_id!("2gmMGMmipfYypLxWsvQ5GQJT5AGnMWmk4Rb9vQMRo6ig");

/// A fully collateralized, physically settled options venue.
///
/// A writer posts the whole of what the holder could ever claim (the
/// underlying for a call, the strike in the quote token for a put) and names a
/// premium; a buyer pays the premium and becomes the holder; the holder may
/// exercise at any time before expiry, paying the other side of the trade and
/// taking the collateral; after expiry the writer reclaims whatever was not
/// exercised. Because the collateral is always in the vault, no position can
/// ever be under water, so there is no margin, no liquidator, and no oracle.
/// This is the design PsyOptions shipped for American options on Solana, with
/// one account per option lot in place of its option and writer tokens.
#[program]
pub mod options {
    use super::*;

    /// Create a venue for one underlying/quote pair. The signer becomes the
    /// admin: the only party who can sweep the venue's fees, and a party who
    /// can do nothing else.
    pub fn initialize_market(
        context: &mut Context<InitializeMarketAccountConstraints>,
        fee_bps: u16,
    ) -> Result<()> {
        instructions::handle_initialize_market(context, fee_bps)
    }

    /// Write a lot of options: post the full collateral and list the lot at
    /// the premium in `terms`. `id` is chosen by the writer so they can have
    /// many open.
    pub fn write_option(
        context: &mut Context<WriteOptionAccountConstraints>,
        id: u64,
        terms: OptionTerms,
    ) -> Result<()> {
        instructions::handle_write_option(context, id, terms)
    }

    /// Buy a listed lot: pay the premium (the venue's fee comes out of it,
    /// the rest goes to the writer) and become the holder.
    pub fn buy_option(context: &mut Context<BuyOptionAccountConstraints>) -> Result<()> {
        instructions::handle_buy_option(context)
    }

    /// Writer withdraws an unsold lot: collateral back, account closed.
    pub fn cancel_option(context: &mut Context<CancelOptionAccountConstraints>) -> Result<()> {
        instructions::handle_cancel_option(context)
    }

    /// Holder exercises before expiry: pays the other side of the trade into
    /// the vault and takes the collateral. Physical settlement: the tokens
    /// change hands, and no price feed is consulted.
    pub fn exercise_option(context: &mut Context<ExerciseOptionAccountConstraints>) -> Result<()> {
        instructions::handle_exercise_option(context)
    }

    /// Writer collects what the holder paid at exercise, and the account
    /// closes.
    pub fn collect_proceeds(
        context: &mut Context<CollectProceedsAccountConstraints>,
    ) -> Result<()> {
        instructions::handle_collect_proceeds(context)
    }

    /// Writer reclaims the collateral of a sold lot the holder let expire, and
    /// the account closes. The premium was theirs the moment it was paid.
    pub fn reclaim_collateral(
        context: &mut Context<ReclaimCollateralAccountConstraints>,
    ) -> Result<()> {
        instructions::handle_reclaim_collateral(context)
    }

    /// Admin sweeps the accumulated premium fees from the quote vault.
    pub fn collect_fees(context: &mut Context<CollectFeesAccountConstraints>) -> Result<()> {
        instructions::handle_collect_fees(context)
    }
}
