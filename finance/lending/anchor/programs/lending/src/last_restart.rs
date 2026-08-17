//! The LastRestartSlot sysvar: the slot of the most recent cluster restart,
//! or 0 if the cluster has never restarted (SIMD-0047). anchor-lang v2 is
//! built on pinocchio, which ships only the Clock and Rent sysvars, so this
//! program declares the 8-byte layout itself and reads it through
//! `pinocchio::sysvars::get_sysvar`, the same syscall wrapper pinocchio's own
//! sysvars use. (`solana-sysvar`'s `LastRestartSlot::get` is not usable here:
//! it is bound to that crate's `Sysvar` trait, not pinocchio's.)
//!
//! Why the program reads it: a halt stops the slot count but not the wall
//! clock, so after a restart an oracle price can look fresh in slots while
//! its value is hours old. `PriceFeed::price_scaled` rejects any price stamped
//! at or before the restart slot, so the market pauses valuation until the
//! publisher posts again.

use anchor_lang::prelude::*;

/// `SysvarLastRestartS1ot1111111111111111111111`, decoded at compile time.
const LAST_RESTART_SLOT_ID: Address =
    anchor_lang::address!("SysvarLastRestartS1ot1111111111111111111111");

/// The sysvar's whole data: one little-endian u64.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct LastRestartSlot {
    pub last_restart_slot: [u8; 8],
}

const _: () = assert!(core::mem::size_of::<LastRestartSlot>() == 8);

impl LastRestartSlot {
    /// Slot of the most recent cluster restart, 0 if there has never been one.
    pub fn last_restart_slot(&self) -> u64 {
        u64::from_le_bytes(self.last_restart_slot)
    }

    pub fn get() -> Result<Self> {
        // `pinocchio::sysvars::get_sysvar` is the safe wrapper over the
        // `sol_get_sysvar` syscall. Off-chain (IDL builds, client compilation)
        // it is a no-op that leaves the buffer zeroed, which reads as "the
        // cluster has never restarted".
        let mut last_restart_slot = [0u8; 8];
        pinocchio::sysvars::get_sysvar(&mut last_restart_slot, &LAST_RESTART_SLOT_ID, 0)?;
        Ok(Self { last_restart_slot })
    }
}
