// The Anchor `#[program]` macro expands to code that clippy flags as a
// diverging sub-expression; this allow is the accepted workaround in this repo.
#![allow(clippy::diverging_sub_expression)]

pub use crate::errors::GameErrorCode;
pub use anchor_lang::prelude::*;
pub use session_keys::{session_auth_or, Session, SessionError};
pub mod constants;
pub mod errors;
pub mod instructions;
pub mod state;
use instructions::*;

// WARNING: This example depends on the `session-keys` crate, which has not
// been independently audited. It is included here purely for educational
// purposes - demonstrating how a game might let a player sign with a
// short-lived session token instead of their main wallet. Do not ship this
// crate (or this program) to mainnet in its current form: review the upstream
// `session-keys` source, get an audit, and harden the session-token issuance
// and expiry handling first.
declare_id!("9aZZ7TJ2fQZxY8hMtWXywp5y6BgqC4N2BPcr9FDT47sW");

#[program]
pub mod extension_nft {
    use super::*;

    pub fn init_player(context: &mut Context<InitPlayerAccountConstraints>, _level_seed: String) -> Result<()> {
        init_player::handle_init_player(context)
    }

    // This function lets the player chop a tree and get 1 wood. The session_auth_or macro
    // lets the player either use their session token or their main wallet. (The counter is only
    // there so that the player can do multiple transactions in the same block. Without it multiple transactions
    // in the same block would result in the same signature and therefore fail.)
    // NOTE: the `#[session_auth_or]` macro injects code that refers to the
    // context binding by the literal name `ctx`, so this handler's context
    // parameter must be named `ctx` (not `context`) for the macro to expand.
    #[session_auth_or(
        ctx.accounts.player.authority.address() == ctx.accounts.signer.address(),
        GameErrorCode::WrongAuthority
    )]
    pub fn chop_tree(ctx: &mut Context<ChopTreeAccountConstraints>, _level_seed: String, counter: u16) -> Result<()> {
        chop_tree::chop_tree(ctx, counter, 1)
    }

    pub fn mint_nft(context: &mut Context<MintNftAccountConstraints>) -> Result<()> {
        mint_nft::handle_mint_nft(context)
    }
}
