use pinocchio::{account_info::AccountInfo, pubkey::Pubkey};
use pinocchio_pubkey::from_str;

pub const TOKEN_EXTENSIONS_PROGRAM_ID: Pubkey = from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");

pub const EXTRA_METAS_SEED: &[u8] = b"extra-account-metas";

const MINT_LEN: usize = 82;
const EXTENSIONS_PADDING: usize = 83;
const EXTENSION_START_OFFSET: usize = 1;
const EXTENSION_LENGTH_LEN: usize = 2;
const EXTENSION_TYPE_LEN: usize = 2;
const TRANSFER_HOOK_EXTENSION_TYPE: u16 = 14;
const IMMUTABLE_OWNER_EXTENSION_TYPE: u16 = 7;
const TYPE_BYTE_OFFSET: usize = MINT_LEN + EXTENSIONS_PADDING;
const EXTENSION_DATA_OFFSET: usize = TYPE_BYTE_OFFSET + EXTENSION_START_OFFSET;
const MINT_TYPE_BYTE: u8 = 1;

pub fn get_transfer_hook_authority(acc_data_bytes: &[u8]) -> Option<&Pubkey> {
    let data = get_extension_data_(acc_data_bytes, TRANSFER_HOOK_EXTENSION_TYPE)?;
    if data.len() < core::mem::size_of::<Pubkey>() {
        return None;
    }
    // The authority is the first 32 bytes of the TransferHook extension data.
    // `Pubkey` is `[u8; 32]` so this cast is a plain reinterpret of the bytes
    // with no alignment requirement.
    Some(unsafe { &*(data.as_ptr() as *const Pubkey) })
}

fn get_extension_data_(acc_data_bytes: &[u8], extension_type: u16) -> Option<&[u8]> {
    // Account data may be shorter than the extension region when the account
    // isn't actually a Token Extensions account; bail safely instead of panicking on
    // the slice operation.
    if acc_data_bytes.len() <= EXTENSION_DATA_OFFSET {
        return None;
    }
    let ext_bytes = &acc_data_bytes[EXTENSION_DATA_OFFSET..];
    let mut start = 0;
    let end = ext_bytes.len();
    while start + EXTENSION_TYPE_LEN + EXTENSION_LENGTH_LEN <= end {
        let ext_type_idx = start;
        let ext_len_idx = ext_type_idx + EXTENSION_TYPE_LEN;
        let ext_data_idx = ext_len_idx + EXTENSION_LENGTH_LEN;

        // Use unaligned reads. The extension TLV header is not guaranteed to
        // be 2-byte aligned inside the runtime-provided account buffer, and
        // SBF (like x86_64) tolerates unaligned reads but doing it via a raw
        // `&*(ptr as *const u16)` is undefined behaviour and can produce
        // garbage when the optimiser folds the read with surrounding ops.
        let ext_type = u16::from_le_bytes([
            ext_bytes[ext_type_idx],
            ext_bytes[ext_type_idx + 1],
        ]);
        let ext_len = u16::from_le_bytes([
            ext_bytes[ext_len_idx],
            ext_bytes[ext_len_idx + 1],
        ]) as usize;

        if ext_data_idx + ext_len > end {
            return None;
        }

        if ext_type == extension_type {
            return Some(&ext_bytes[ext_data_idx..ext_data_idx + ext_len]);
        }

        start = ext_data_idx + ext_len;
    }
    None
}

pub fn has_immutable_owner_extension(acc_data_bytes: &[u8]) -> bool {
    let extension_data = get_extension_data_(acc_data_bytes, IMMUTABLE_OWNER_EXTENSION_TYPE);
    extension_data.is_some()
}

pub fn is_token_extensions_mint(mint: &AccountInfo) -> bool {
    // Order of checks matters: read the type byte ONLY after we have proven
    // the buffer is long enough. The previous implementation indexed first
    // and length-checked second, which faulted (out-of-bounds) on any account
    // shorter than 166 bytes — every mint that isn't a Token Extensions mint hits this.
    if !mint.is_owned_by(&TOKEN_EXTENSIONS_PROGRAM_ID) {
        return false;
    }
    let data = unsafe { mint.borrow_data_unchecked() };
    if data.len() <= TYPE_BYTE_OFFSET {
        return false;
    }
    data[TYPE_BYTE_OFFSET] == MINT_TYPE_BYTE
}
