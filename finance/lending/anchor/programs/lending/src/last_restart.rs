//! The LastRestartSlot sysvar: the slot of the most recent cluster restart,
//! or 0 if the cluster has never restarted (SIMD-0047). anchor-lang v2 is
//! built on pinocchio, which ships only the Clock and Rent sysvars, so this
//! program declares the 8-byte layout itself and reads it through the same
//! `sol_get_sysvar` syscall pinocchio's own sysvars use. (`solana-sysvar`'s
//! `LastRestartSlot::get` is not usable here: it is bound to that crate's
//! `Sysvar` trait, not pinocchio's.)
//!
//! Why the program reads it: a halt stops the slot count but not the wall
//! clock, so after a restart an oracle price can look fresh in slots while
//! its value is hours old. `PriceFeed::price_scaled` rejects any price stamped
//! at or before the restart slot, so the market pauses valuation until the
//! publisher posts again.

use anchor_lang::prelude::*;

/// `SysvarLastRestartS1ot1111111111111111111111`, decoded at compile time.
/// Only the on-chain branch of `get` reads it.
#[cfg_attr(not(any(target_os = "solana", target_arch = "bpf")), allow(dead_code))]
const LAST_RESTART_SLOT_ID: Address =
    anchor_lang::address!("SysvarLastRestartS1ot1111111111111111111111");

/// The sysvar's whole data: one little-endian u64.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct LastRestartSlot {
    pub last_restart_slot: [u8; 8],
}

const _: () = assert!(core::mem::size_of::<LastRestartSlot>() == 8);
const _: () = assert!(core::mem::align_of::<LastRestartSlot>() == 1);

impl LastRestartSlot {
    /// Slot of the most recent cluster restart, 0 if there has never been one.
    pub fn last_restart_slot(&self) -> u64 {
        u64::from_le_bytes(self.last_restart_slot)
    }

    pub fn get() -> Result<Self> {
        let mut var = core::mem::MaybeUninit::<Self>::uninit();
        let var_addr = var.as_mut_ptr() as *mut u8;

        #[cfg(any(target_os = "solana", target_arch = "bpf"))]
        // SAFETY: `var_addr` points at 8 writable bytes and the syscall
        // writes exactly 8.
        let result = unsafe {
            solana_define_syscall::definitions::sol_get_sysvar(
                &LAST_RESTART_SLOT_ID as *const _ as *const u8,
                var_addr,
                0,
                core::mem::size_of::<Self>() as u64,
            )
        };

        // Off-chain (IDL builds, client compilation) the sysvar reads as
        // zero: the cluster has never restarted.
        #[cfg(not(any(target_os = "solana", target_arch = "bpf")))]
        let result: u64 = {
            // SAFETY: `var_addr` points at 8 writable bytes.
            unsafe { var_addr.write_bytes(0, core::mem::size_of::<Self>()) };
            0
        };

        match result {
            // SAFETY: on success the syscall (or the zeroing above) has
            // initialized all 8 bytes.
            0 => Ok(unsafe { var.assume_init() }),
            _ => Err(ProgramError::UnsupportedSysvar),
        }
    }
}
