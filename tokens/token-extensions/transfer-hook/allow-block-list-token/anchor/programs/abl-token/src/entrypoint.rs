//! Program entrypoint, hand-written so the SPL transfer-hook interface reaches
//! `tx_hook`.
//!
//! Anchor v2 gives every instruction in an executable `#[program]` the eight-byte
//! `sha256("global:<name>")` discriminator, and `#[discrim = N]` there is limited
//! to a single byte. The transfer-hook interface calls `Execute` under its own
//! eight-byte value, which leaves no way to declare that handler directly.
//! `#[program(interface, ...)]` accepts arbitrary discriminator bytes but only
//! generates a CPI client: no dispatch, and so no deployable program.
//!
//! So the crate builds with `no-entrypoint` (which makes anchor export its
//! dispatch as `__anchor_dispatch` instead of claiming the `entrypoint` symbol)
//! and this module claims `entrypoint` itself. All it does is swap the
//! interface's discriminator for `tx_hook`'s before delegating; the payload
//! behind it (a single `u64` amount) is identical either way.

use anchor_lang::pinocchio;

pinocchio::default_allocator!();
pinocchio::default_panic_handler!();

/// `sha256("spl-transfer-hook-interface:execute")[..8]`, the discriminator
/// Token-2022 uses when it calls a mint's transfer hook.
const EXECUTE_DISCRIMINATOR: [u8; 8] = [105, 37, 101, 197, 75, 251, 102, 26];

/// `sha256("global:tx_hook")[..8]`, what anchor's dispatch matches on.
const TX_HOOK_DISCRIMINATOR: [u8; 8] = [55, 222, 121, 59, 26, 10, 108, 168];

/// # Safety
///
/// Called only by the SBF loader, with the register convention anchor's own
/// entrypoint documents: `r1` is the start of the serialized parameter region
/// and `r2` points at the instruction data, whose length sits in the eight
/// bytes below it.
#[cfg(target_os = "solana")]
#[no_mangle]
pub unsafe extern "C" fn entrypoint(input: *mut u8, ix_data_ptr: *const u8) -> u64 {
    let len = *(ix_data_ptr.sub(8) as *const u64) as usize;
    if len >= EXECUTE_DISCRIMINATOR.len() {
        let discriminator = core::slice::from_raw_parts_mut(
            ix_data_ptr as *mut u8,
            EXECUTE_DISCRIMINATOR.len(),
        );
        if discriminator == EXECUTE_DISCRIMINATOR {
            discriminator.copy_from_slice(&TX_HOOK_DISCRIMINATOR);
        }
    }
    crate::__anchor_dispatch(input, ix_data_ptr)
}
