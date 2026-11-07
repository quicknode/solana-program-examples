use quasar_lang::prelude::*;

/// Message account with a dynamic-length message field.
/// Quasar's `set_inner` automatically reallocs when the new message exceeds
/// the current account size, making explicit realloc unnecessary.
#[account(discriminator = 1, set_inner)]
pub struct MessageAccount {
    pub message: String<1024, 2>,
}
