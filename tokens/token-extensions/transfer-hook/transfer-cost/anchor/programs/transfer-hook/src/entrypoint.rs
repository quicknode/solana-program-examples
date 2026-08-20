//! Program entrypoint, hand-written so the SPL transfer-hook interface reaches
//! this program's handlers.
//!
//! Anchor v2 gives every instruction in an executable `#[program]` the eight-byte
//! `sha256("global:<name>")` discriminator, and `#[discrim = N]` there is limited
//! to a single byte. The transfer-hook interface calls its instructions under
//! their own eight-byte values, which leaves no way to declare those handlers
//! directly. `#[program(interface, ...)]` accepts arbitrary discriminator bytes
//! but only generates a CPI client: no dispatch, and so no deployable program.
//!
//! So the crate builds with `no-entrypoint` (which makes anchor export its
//! dispatch as `__anchor_dispatch` instead of claiming the `entrypoint` symbol)
//! and this module claims `entrypoint` itself. All it does is swap an interface
//! discriminator for the matching handler's before delegating; the payload
//! behind it is identical either way.

use anchor_lang::pinocchio;

pinocchio::default_allocator!();
pinocchio::default_panic_handler!();

/// Interface discriminator paired with the handler's own, in declaration order.
// Read only by `entrypoint` below, which the host build cfgs out.
#[cfg(target_os = "solana")]
const DISCRIMINATOR_MAP: [([u8; 8], [u8; 8]); 2] = [
    // initialize_extra_account_meta_list
    (
        [43, 34, 13, 49, 167, 88, 235, 235],
        [92, 197, 174, 197, 41, 124, 19, 3],
    ),
    // transfer_hook
    (
        [105, 37, 101, 197, 75, 251, 102, 26],
        [220, 57, 220, 152, 126, 125, 97, 168],
    ),
];

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
    if len >= 8 {
        let discriminator = core::slice::from_raw_parts_mut(ix_data_ptr as *mut u8, 8);
        for (interface, handler) in DISCRIMINATOR_MAP {
            if discriminator == interface {
                discriminator.copy_from_slice(&handler);
                break;
            }
        }
    }
    crate::__anchor_dispatch(input, ix_data_ptr)
}
