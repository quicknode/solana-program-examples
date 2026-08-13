#![allow(clippy::diverging_sub_expression)]

use anchor_lang::prelude::*;

use instructions::*;
pub mod instructions;

declare_id!("BJHEDXSQfD9kBFvhw8ZCGmPFRihzvbMoxoHUKpXdpn4D");

#[program]
pub mod metadata {
    use super::*;

    pub fn initialize(context: &mut Context<InitializeAccountConstraints>, args: TokenMetadataArgs) -> Result<()> {
        process_initialize(context, args)
    }

    pub fn update_field(context: &mut Context<UpdateFieldAccountConstraints>, args: UpdateFieldArgs) -> Result<()> {
        process_update_field(context, args)
    }

    pub fn remove_key(context: &mut Context<RemoveKeyAccountConstraints>, key: String) -> Result<()> {
        process_remove_key(context, key)
    }

    pub fn emit(context: &mut Context<EmitAccountConstraints>) -> Result<()> {
        process_emit(context)
    }

    pub fn update_authority(context: &mut Context<UpdateAuthorityAccountConstraints>) -> Result<()> {
        process_update_authority(context)
    }
}
