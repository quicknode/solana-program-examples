#![cfg_attr(not(test), no_std)]

use quasar_lang::sysvars::Sysvar;
use quasar_lang::{cpi::Seed, prelude::*};

#[cfg(test)]
mod tests;

declare_id!("DrWbQtYJGtsoRwzKqAbHKHKsCJJfpysudF39GBVFSxub");

/// SPL Transfer Hook Interface discriminators (SHA-256 prefix).
#[allow(dead_code)]
const EXECUTE_DISCRIMINATOR: [u8; 8] = [105, 37, 101, 197, 75, 251, 102, 26];

const EXTRA_ACCOUNT_METAS_SEED: &[u8] = b"extra-account-metas";
const ALLOW_LIST_SEED: &[u8] = b"allow_list";
const ALLOWED_WALLET_SEED: &[u8] = b"allowed_wallet";

/// AllowList layout: [32 bytes authority] [1 byte bump].
const ALLOW_LIST_SIZE: u64 = 33;
/// AllowedWallet layout: [1 byte marker, 1 while the wallet is allowed] [1 byte bump].
const ALLOWED_WALLET_SIZE: u64 = 2;

/// Size of the ExtraAccountMetaList account holding one extra account:
/// 8 (discriminator) + 4 (length) + 4 (count) + 35 (one ExtraAccountMeta).
const META_LIST_SIZE: u64 = 51;

// Base SPL layouts: a mint's `mint_authority` is a COption<Pubkey> at bytes
// 0..36 (4-byte tag, then the key), and a token account's owner sits at 32..64.
const MINT_AUTHORITY_TAG: core::ops::Range<usize> = 0..4;
const MINT_AUTHORITY_KEY: core::ops::Range<usize> = 4..36;
const TOKEN_ACCOUNT_OWNER: core::ops::Range<usize> = 32..64;

const ERROR_RECIPIENT_NOT_ALLOWED: u32 = 6000;
const ERROR_UNAUTHORIZED: u32 = 6001;

/// Transfer hook that only lets a mint's tokens reach wallets on that mint's
/// allow list. Each allowed wallet is its own PDA,
/// ["allowed_wallet", mint, wallet], so the list has no size limit.
#[program]
mod quasar_transfer_hook_allow_list {
    use super::*;

    /// Create the ExtraAccountMetaList PDA and the mint's AllowList PDA. Only
    /// the mint authority can call it, and becomes the allow list's authority.
    /// Discriminator = sha256("spl-transfer-hook-interface:initialize-extra-account-metas")[:8]
    #[instruction(discriminator = [43, 34, 13, 49, 167, 88, 235, 235])]
    pub fn initialize_extra_account_meta_list(
        ctx: Ctx<InitializeExtraAccountMetaListAccountConstraints>,
    ) -> Result<(), ProgramError> {
        handle_initialize(&mut ctx.accounts)
    }

    /// Transfer hook handler - checks the recipient wallet is allowed.
    /// Discriminator = sha256("spl-transfer-hook-interface:execute")[:8]
    #[instruction(discriminator = [105, 37, 101, 197, 75, 251, 102, 26])]
    pub fn transfer_hook(
        ctx: Ctx<TransferHookAccountConstraints>,
        _amount: u64,
    ) -> Result<(), ProgramError> {
        handle_transfer_hook(&ctx.accounts)
    }

    /// Allow a wallet to receive the mint's tokens. Only the allow list's authority.
    #[instruction(discriminator = [0, 0, 0, 0, 0, 0, 0, 2])]
    pub fn allow_wallet(ctx: Ctx<AllowWalletAccountConstraints>) -> Result<(), ProgramError> {
        handle_allow_wallet(&mut ctx.accounts)
    }

    /// Remove a wallet from the allow list, closing its PDA. Only the allow list's authority.
    #[instruction(discriminator = [0, 0, 0, 0, 0, 0, 0, 3])]
    pub fn disallow_wallet(ctx: Ctx<DisallowWalletAccountConstraints>) -> Result<(), ProgramError> {
        handle_disallow_wallet(&mut ctx.accounts)
    }
}

fn recipient_not_allowed() -> ProgramError {
    ProgramError::Custom(ERROR_RECIPIENT_NOT_ALLOWED)
}

fn unauthorized() -> ProgramError {
    ProgramError::Custom(ERROR_UNAUTHORIZED)
}

/// Check `account` is the mint's AllowList PDA and `signer` its authority.
fn require_allow_list_authority(
    allow_list: &UncheckedAccount,
    mint: &Address,
    signer: &Address,
) -> Result<(), ProgramError> {
    let (allow_list_pda, _) =
        quasar_lang::pda::try_find_program_address(&[ALLOW_LIST_SEED, mint.as_ref()], &crate::ID)?;
    let view = allow_list.to_account_view();
    if view.address() != &allow_list_pda || !view.owned_by(&crate::ID) {
        return Err(ProgramError::InvalidSeeds);
    }
    let data = view.try_borrow()?;
    if data.len() < ALLOW_LIST_SIZE as usize {
        return Err(ProgramError::UninitializedAccount);
    }
    if &data[0..32] != signer.as_ref() {
        log("Only the allow list's authority can change it");
        return Err(unauthorized());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// InitializeExtraAccountMetaList
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitializeExtraAccountMetaListAccountConstraints {
    #[account(mut)]
    pub payer: Signer,
    #[account(mut)]
    pub extra_account_meta_list: UncheckedAccount,
    pub mint: UncheckedAccount,
    /// AllowList PDA: ["allow_list", mint]
    #[account(mut)]
    pub allow_list: UncheckedAccount,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_initialize(
    accounts: &mut InitializeExtraAccountMetaListAccountConstraints,
) -> Result<(), ProgramError> {
    let mint_address = *accounts.mint.to_account_view().address();
    let payer_address = *accounts.payer.to_account_view().address();

    // Only the mint authority may set up the mint's allow list, so nobody can
    // claim the list for a mint they do not issue.
    {
        let mint_data = accounts.mint.to_account_view().try_borrow()?;
        if mint_data.len() < MINT_AUTHORITY_KEY.end
            || mint_data[MINT_AUTHORITY_TAG] != 1u32.to_le_bytes()
            || &mint_data[MINT_AUTHORITY_KEY] != payer_address.as_ref()
        {
            log("Only the mint authority can create the allow list");
            return Err(unauthorized());
        }
    }

    // Create ExtraAccountMetaList PDA (1 extra account: the recipient's AllowedWallet)
    let (meta_list_pda, meta_list_bump) = quasar_lang::pda::try_find_program_address(
        &[EXTRA_ACCOUNT_METAS_SEED, mint_address.as_ref()],
        &crate::ID,
    )?;
    if accounts.extra_account_meta_list.to_account_view().address() != &meta_list_pda {
        return Err(ProgramError::InvalidSeeds);
    }
    let meta_list_bump_bytes = [meta_list_bump];
    let meta_list_seeds = [
        Seed::from(EXTRA_ACCOUNT_METAS_SEED),
        Seed::from(mint_address.as_ref()),
        Seed::from(&meta_list_bump_bytes as &[u8]),
    ];
    accounts
        .system_program
        .create_account(
            &accounts.payer,
            &accounts.extra_account_meta_list,
            Rent::get()?.try_minimum_balance(META_LIST_SIZE as usize)?,
            META_LIST_SIZE,
            &crate::ID,
        )
        .invoke_signed(&meta_list_seeds)?;

    {
        let view = unsafe {
            &mut *(&mut accounts.extra_account_meta_list as *mut UncheckedAccount
                as *mut AccountView)
        };
        let mut data = view.try_borrow_mut()?;
        data[0..8].copy_from_slice(&EXECUTE_DISCRIMINATOR);
        data[8..12].copy_from_slice(&39u32.to_le_bytes());
        data[12..16].copy_from_slice(&1u32.to_le_bytes());

        // ExtraAccountMeta: PDA of this program from three seeds,
        // [Literal("allowed_wallet"), AccountKey(1) = mint,
        //  AccountData(2, 32, 32) = the destination token account's owner].
        data[16] = 1; // PDA from seeds
        let mut config = [0u8; 32];
        let literal_len = ALLOWED_WALLET_SEED.len();
        config[0] = 1; // literal
        config[1] = literal_len as u8;
        config[2..2 + literal_len].copy_from_slice(ALLOWED_WALLET_SEED);
        let mut cursor = 2 + literal_len;
        config[cursor..cursor + 2].copy_from_slice(&[3, 1]); // account key, index 1
        cursor += 2;
        config[cursor..cursor + 4].copy_from_slice(&[4, 2, 32, 32]); // account data, index 2, offset 32, length 32
        data[17..49].copy_from_slice(&config);
        data[49] = 0; // not signer
        data[50] = 0; // not writable
    }

    // Create the mint's AllowList PDA, with the mint authority as its authority.
    let (allow_list_pda, allow_list_bump) = quasar_lang::pda::try_find_program_address(
        &[ALLOW_LIST_SEED, mint_address.as_ref()],
        &crate::ID,
    )?;
    if accounts.allow_list.to_account_view().address() != &allow_list_pda {
        return Err(ProgramError::InvalidSeeds);
    }
    let allow_list_bump_bytes = [allow_list_bump];
    let allow_list_seeds = [
        Seed::from(ALLOW_LIST_SEED),
        Seed::from(mint_address.as_ref()),
        Seed::from(&allow_list_bump_bytes as &[u8]),
    ];
    accounts
        .system_program
        .create_account(
            &accounts.payer,
            &accounts.allow_list,
            Rent::get()?.try_minimum_balance(ALLOW_LIST_SIZE as usize)?,
            ALLOW_LIST_SIZE,
            &crate::ID,
        )
        .invoke_signed(&allow_list_seeds)?;

    let view =
        unsafe { &mut *(&mut accounts.allow_list as *mut UncheckedAccount as *mut AccountView) };
    let mut data = view.try_borrow_mut()?;
    data[0..32].copy_from_slice(payer_address.as_ref());
    data[32] = allow_list_bump;

    log("Allow list transfer hook initialized");
    Ok(())
}

// ---------------------------------------------------------------------------
// TransferHook
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct TransferHookAccountConstraints {
    pub source_token: UncheckedAccount,
    pub mint: UncheckedAccount,
    pub destination_token: UncheckedAccount,
    pub owner: UncheckedAccount,
    pub extra_account_meta_list: UncheckedAccount,
    /// The recipient's AllowedWallet PDA. It may not exist: a missing account
    /// is how a wallet that is not allowed shows up.
    pub allowed_wallet: UncheckedAccount,
}

#[inline(always)]
pub fn handle_transfer_hook(accounts: &TransferHookAccountConstraints) -> Result<(), ProgramError> {
    // The recipient is the wallet that owns the destination token account.
    let destination = accounts.destination_token.to_account_view().try_borrow()?;
    if destination.len() < TOKEN_ACCOUNT_OWNER.end {
        return Err(ProgramError::InvalidAccountData);
    }
    let recipient = &destination[TOKEN_ACCOUNT_OWNER];

    let mint_address = accounts.mint.to_account_view().address();
    let (allowed_wallet_pda, _) = quasar_lang::pda::try_find_program_address(
        &[ALLOWED_WALLET_SEED, mint_address.as_ref(), recipient],
        &crate::ID,
    )?;
    let allowed_wallet = accounts.allowed_wallet.to_account_view();
    if allowed_wallet.address() != &allowed_wallet_pda {
        return Err(ProgramError::InvalidSeeds);
    }

    // Only this program can create an account at one of its own PDAs, so a
    // live, program-owned account here was created by `allow_wallet` and not
    // since closed by `disallow_wallet`.
    let allowed = allowed_wallet.owned_by(&crate::ID)
        && allowed_wallet.data_len() >= ALLOWED_WALLET_SIZE as usize
        && allowed_wallet.try_borrow()?[0] == 1;
    if !allowed {
        log("Recipient wallet is not on the allow list");
        return Err(recipient_not_allowed());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// AllowWallet
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct AllowWalletAccountConstraints {
    #[account(mut)]
    pub authority: Signer,
    pub mint: UncheckedAccount,
    pub allow_list: UncheckedAccount,
    pub wallet: UncheckedAccount,
    #[account(mut)]
    pub allowed_wallet: UncheckedAccount,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_allow_wallet(
    accounts: &mut AllowWalletAccountConstraints,
) -> Result<(), ProgramError> {
    let mint_address = *accounts.mint.to_account_view().address();
    let wallet_address = *accounts.wallet.to_account_view().address();
    require_allow_list_authority(
        &accounts.allow_list,
        &mint_address,
        accounts.authority.to_account_view().address(),
    )?;

    let (allowed_wallet_pda, bump) = quasar_lang::pda::try_find_program_address(
        &[
            ALLOWED_WALLET_SEED,
            mint_address.as_ref(),
            wallet_address.as_ref(),
        ],
        &crate::ID,
    )?;
    if accounts.allowed_wallet.to_account_view().address() != &allowed_wallet_pda {
        return Err(ProgramError::InvalidSeeds);
    }
    let bump_bytes = [bump];
    let seeds = [
        Seed::from(ALLOWED_WALLET_SEED),
        Seed::from(mint_address.as_ref()),
        Seed::from(wallet_address.as_ref()),
        Seed::from(&bump_bytes as &[u8]),
    ];
    accounts
        .system_program
        .create_account(
            &accounts.authority,
            &accounts.allowed_wallet,
            Rent::get()?.try_minimum_balance(ALLOWED_WALLET_SIZE as usize)?,
            ALLOWED_WALLET_SIZE,
            &crate::ID,
        )
        .invoke_signed(&seeds)?;

    let view = unsafe {
        &mut *(&mut accounts.allowed_wallet as *mut UncheckedAccount as *mut AccountView)
    };
    let mut data = view.try_borrow_mut()?;
    data[0] = 1;
    data[1] = bump;

    log("Wallet allowed");
    Ok(())
}

// ---------------------------------------------------------------------------
// DisallowWallet
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct DisallowWalletAccountConstraints {
    #[account(mut)]
    pub authority: Signer,
    pub mint: UncheckedAccount,
    pub allow_list: UncheckedAccount,
    pub wallet: UncheckedAccount,
    #[account(mut)]
    pub allowed_wallet: UncheckedAccount,
}

#[inline(always)]
pub fn handle_disallow_wallet(
    accounts: &mut DisallowWalletAccountConstraints,
) -> Result<(), ProgramError> {
    let mint_address = *accounts.mint.to_account_view().address();
    let wallet_address = *accounts.wallet.to_account_view().address();
    require_allow_list_authority(
        &accounts.allow_list,
        &mint_address,
        accounts.authority.to_account_view().address(),
    )?;

    let (allowed_wallet_pda, _) = quasar_lang::pda::try_find_program_address(
        &[
            ALLOWED_WALLET_SEED,
            mint_address.as_ref(),
            wallet_address.as_ref(),
        ],
        &crate::ID,
    )?;
    let allowed_wallet = accounts.allowed_wallet.to_account_view();
    if allowed_wallet.address() != &allowed_wallet_pda || !allowed_wallet.owned_by(&crate::ID) {
        return Err(ProgramError::InvalidSeeds);
    }

    // Close the AllowedWallet account: rent back to the authority, data zeroed.
    let refund = allowed_wallet.lamports();
    let authority = accounts.authority.to_account_view();
    set_lamports(allowed_wallet, 0);
    set_lamports(authority, authority.lamports() + refund);

    let view = unsafe {
        &mut *(&mut accounts.allowed_wallet as *mut UncheckedAccount as *mut AccountView)
    };
    let mut data = view.try_borrow_mut()?;
    for byte in data.iter_mut() {
        *byte = 0;
    }

    log("Wallet disallowed");
    Ok(())
}
