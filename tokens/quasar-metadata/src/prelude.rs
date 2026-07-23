//! Convenience re-exports for metadata programs.
//!
//! ```rust,ignore
//! use quasar_lang::prelude::*;
//! use quasar_metadata::prelude::*;
//! ```

pub use crate::{
    accounts::{master_edition, metadata},
    init::{InitMasterEdition, InitMetadata},
    instructions::MetadataCpi,
    pda::*,
    MasterEditionAccount, MasterEditionPrefix, MasterEditionPrefixZc, MetadataAccount,
    MetadataPrefix, MetadataPrefixZc, MetadataProgram,
};
