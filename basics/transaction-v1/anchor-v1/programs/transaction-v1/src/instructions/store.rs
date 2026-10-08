use crate::{state::Document, DocumentError};
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(document: Vec<u8>)]
pub struct StoreDocumentAccountConstraints<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// Sized to this document exactly, so the rent matches what was stored.
    #[account(
        init,
        payer = payer,
        space = Document::required_space(document.len()),
        seeds = [
            Document::SEED_PREFIX,
            payer.key().as_ref(),
        ],
        bump,
    )]
    pub document_account: Account<'info, Document>,
    pub system_program: Program<'info, System>,
}

pub fn handle_store_document(
    context: Context<StoreDocumentAccountConstraints>,
    document: Vec<u8>,
) -> Result<()> {
    require!(!document.is_empty(), DocumentError::EmptyDocument);
    context.accounts.document_account.data = document;
    Ok(())
}
