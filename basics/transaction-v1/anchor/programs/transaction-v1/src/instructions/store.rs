// v2's `#[derive(Accounts)]` binds the `#[instruction(...)]` args in more
// than one generated item, and only the one evaluating the constraints below
// reads them, so the binding looks unused to rustc even though `space` uses it.
#![allow(unused_variables)]

use crate::{state::Document, DocumentError};
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(document: Vec<u8>)]
pub struct StoreDocumentAccountConstraints {
    #[account(mut)]
    pub payer: Signer,

    /// Sized to this document exactly, so the rent matches what was stored.
    #[account(
        init,
        payer = payer,
        space = Document::required_space(document.len()),
        seeds = [
            Document::SEED_PREFIX,
            payer.address().as_ref(),
        ],
        bump,
    )]
    pub document_account: BorshAccount<Document>,
    pub system_program: Program<System>,
}

pub fn handle_store_document(
    context: &mut Context<StoreDocumentAccountConstraints>,
    document: Vec<u8>,
) -> Result<()> {
    require!(!document.is_empty(), DocumentError::EmptyDocument);
    context.accounts.document_account.data = document;
    Ok(())
}
