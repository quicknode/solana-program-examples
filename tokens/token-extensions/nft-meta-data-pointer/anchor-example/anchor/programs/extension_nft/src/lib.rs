// The Anchor `#[program]` macro expands to code that clippy flags as a
// diverging sub-expression; this allow is the accepted workaround in this repo.
#![allow(clippy::diverging_sub_expression)]

pub use crate::errors::GameErrorCode;
pub use anchor_lang::prelude::*;
pub mod constants;
pub mod errors;
pub mod instructions;
pub mod session;
pub mod state;
use instructions::*;

// WARNING: this example reads gpl-session's `SessionToken` account (see
// `session.rs`), and that program has not been independently audited. It is
// included here purely for educational purposes - demonstrating how a game
// might let a player sign with a short-lived session token instead of their
// main wallet. Do not ship this pattern to mainnet in its current form: review
// the upstream session-keys source, get an audit, and harden the session-token
// issuance and expiry handling first.
declare_id!("9aZZ7TJ2fQZxY8hMtWXywp5y6BgqC4N2BPcr9FDT47sW");

#[program]
pub mod extension_nft {
    use super::*;

    pub fn init_player(
        context: &mut Context<InitPlayerAccountConstraints>,
        _level_seed: String,
    ) -> Result<()> {
        init_player::handle_init_player(context)
    }

    // This function lets the player chop a tree and get 1 wood. The session_auth_or macro
    // lets the player either use their session token or their main wallet. (The counter is only
    // there so that the player can do multiple transactions in the same block. Without it multiple transactions
    // in the same block would result in the same signature and therefore fail.)
    // The session-keys `#[session_auth_or]` attribute macro is Anchor v1 only,
    // so the same check is spelled out: a live session token authorizes the
    // call, and otherwise the signer has to be the player's own authority.
    pub fn chop_tree(
        ctx: &mut Context<ChopTreeAccountConstraints>,
        _level_seed: String,
        counter: u16,
    ) -> Result<()> {
        let signer = *ctx.accounts.signer.address();
        let authority = ctx.accounts.player.authority;
        let has_session = match ctx.accounts.session_token.as_ref() {
            Some(token) => session::is_valid_session(token, &signer, &authority)?,
            None => false,
        };
        require!(
            has_session || authority == signer,
            GameErrorCode::WrongAuthority
        );

        chop_tree::chop_tree(ctx, counter, 1)
    }

    pub fn mint_nft(context: &mut Context<MintNftAccountConstraints>) -> Result<()> {
        mint_nft::handle_mint_nft(context)
    }
}
