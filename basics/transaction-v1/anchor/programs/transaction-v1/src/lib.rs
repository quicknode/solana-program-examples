//! Store a document of arbitrary size in a program-owned account, in one
//! instruction.
//!
//! The program itself has nothing to do with transaction v1: it reads the
//! accounts and instruction data it is given like any other program, and does
//! not know or care which transaction format carried them. What v1 changes is
//! how much a single instruction can carry: a 3,000 byte document is beyond
//! the 1,232 byte limit of a legacy or v0 transaction, so the tests send it
//! in a v1 transaction (up to 4,096 bytes) instead.

use anchor_lang::prelude::*;
use instructions::*;

pub mod instructions;
pub mod state;

declare_id!("F1dMqenFVwcp3SmX9Sq9e22nmcxcact5No12pmb5rL5F");

#[program]
pub mod transaction_v1_anchor_program {
    use super::*;

    pub fn store_document(
        context: &mut Context<StoreDocumentAccountConstraints>,
        document: Vec<u8>,
    ) -> Result<()> {
        store::handle_store_document(context, document)
    }
}

#[error_code]
pub enum DocumentError {
    #[msg("A document cannot be empty")]
    EmptyDocument,
}
