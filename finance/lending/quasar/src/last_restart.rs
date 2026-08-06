//! The LastRestartSlot sysvar: the slot of the most recent cluster restart,
//! or 0 if the cluster has never restarted (SIMD-0047). quasar-lang ships
//! only the Clock and Rent sysvars, so this program declares the 8-byte
//! layout itself and reads it through the same `sol_get_sysvar` syscall
//! quasar's own sysvars use.
//!
//! Why the program reads it: a halt stops the slot count but not the wall
//! clock, so after a restart an oracle price can look fresh in slots while
//! its value is hours old. `logic::price_scaled` rejects any price stamped
//! at or before the restart slot, pausing valuation until the publisher
//! posts again.

use quasar_lang::{pod::PodU64, prelude::Address, sysvars::Sysvar};
use solana_program_error::ProgramError;

/// `SysvarLastRestartS1ot1111111111111111111111`, decoded at compile time.
const LAST_RESTART_SLOT_ID: Address =
    quasar_lang::prelude::address!("SysvarLastRestartS1ot1111111111111111111111");

/// The sysvar's whole data: one little-endian u64.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct LastRestartSlot {
    pub last_restart_slot: PodU64,
}

const _: () = assert!(core::mem::size_of::<LastRestartSlot>() == 8);
const _: () = assert!(core::mem::align_of::<LastRestartSlot>() == 1);

// Written out by hand rather than with quasar-lang's `impl_sysvar_get!`: the
// macro's expansion names private quasar-lang constants, so it only works
// inside that crate. The sysvar is 8 bytes with no padding, so the syscall
// fills the whole struct.
impl Sysvar for LastRestartSlot {
    const ID: Address = LAST_RESTART_SLOT_ID;

    #[inline(always)]
    unsafe fn from_bytes_unchecked(bytes: &[u8]) -> &Self {
        // SAFETY: the caller guarantees `bytes` holds at least 8 bytes of
        // valid sysvar data; the struct is `#[repr(C)]` with alignment 1,
        // so the pointer cast is always valid.
        unsafe { &*(bytes.as_ptr() as *const Self) }
    }

    fn get() -> Result<Self, ProgramError> {
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
