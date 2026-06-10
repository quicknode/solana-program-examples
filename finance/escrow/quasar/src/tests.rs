extern crate std;
use {
    alloc::vec,
    alloc::vec::Vec,
    quasar_svm::{Account, Instruction, Pubkey, QuasarSvm},
    solana_program_pack::Pack,
    spl_token_interface::state::{Account as TokenAccount, AccountState, Mint},
    std::println,
};

const OFFER_ID: u64 = 7;
const DEPOSIT_AMOUNT: u64 = 1337;
const RECEIVE_AMOUNT: u64 = 1337;
const STARTING_LAMPORTS: u64 = 1_000_000_000;
const OFFER_ACCOUNT_LAMPORTS: u64 = 2_000_000;

fn setup() -> QuasarSvm {
    let elf = std::fs::read("target/deploy/quasar_escrow.so").unwrap();
    QuasarSvm::new()
        .with_program(&crate::ID, &elf)
        .with_token_program()
}

fn signer(address: Pubkey) -> Account {
    quasar_svm::token::create_keyed_system_account(&address, STARTING_LAMPORTS)
}

fn empty(address: Pubkey) -> Account {
    Account {
        address,
        lamports: 0,
        data: vec![],
        owner: quasar_svm::system_program::ID,
        executable: false,
    }
}

fn mint(address: Pubkey, authority: Pubkey) -> Account {
    quasar_svm::token::create_keyed_mint_account(
        &address,
        &Mint {
            mint_authority: Some(authority).into(),
            supply: 1_000_000_000,
            decimals: 9,
            is_initialized: true,
            freeze_authority: None.into(),
        },
    )
}

fn token(address: Pubkey, mint: Pubkey, owner: Pubkey, amount: u64) -> Account {
    quasar_svm::token::create_keyed_token_account(
        &address,
        &TokenAccount {
            mint,
            owner,
            amount,
            state: AccountState::Initialized,
            ..TokenAccount::default()
        },
    )
}

fn token_amount(account: &Account) -> u64 {
    TokenAccount::unpack(&account.data).unwrap().amount
}

fn derive_offer(maker: &Pubkey, id: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"offer", maker.as_ref(), &id.to_le_bytes()], &crate::ID)
}

/// Build offer account data manually.
/// Layout (from #[account] codegen):
///   [disc: 1 byte = 1]
///   [id: 8 bytes (PodU64 LE)]
///   [maker: 32 bytes (Address)]
///   [token_mint_a: 32 bytes]
///   [token_mint_b: 32 bytes]
///   [maker_token_account_b: 32 bytes]
///   [vault: 32 bytes]
///   [receive: 8 bytes (PodU64 LE)]
///   [bump: 1 byte]
/// Total: 178 bytes
#[allow(clippy::too_many_arguments)]
fn offer_data(
    id: u64,
    maker: Pubkey,
    token_mint_a: Pubkey,
    token_mint_b: Pubkey,
    maker_token_account_b: Pubkey,
    vault: Pubkey,
    receive: u64,
    bump: u8,
) -> Vec<u8> {
    let mut data = Vec::with_capacity(178);
    data.push(1u8); // discriminator
    data.extend_from_slice(&id.to_le_bytes());
    data.extend_from_slice(maker.as_ref());
    data.extend_from_slice(token_mint_a.as_ref());
    data.extend_from_slice(token_mint_b.as_ref());
    data.extend_from_slice(maker_token_account_b.as_ref());
    data.extend_from_slice(vault.as_ref());
    data.extend_from_slice(&receive.to_le_bytes());
    data.push(bump);
    data
}

#[allow(clippy::too_many_arguments)]
fn offer_account(
    address: Pubkey,
    id: u64,
    maker: Pubkey,
    token_mint_a: Pubkey,
    token_mint_b: Pubkey,
    maker_token_account_b: Pubkey,
    vault: Pubkey,
    receive: u64,
    bump: u8,
) -> Account {
    Account {
        address,
        lamports: OFFER_ACCOUNT_LAMPORTS,
        data: offer_data(
            id,
            maker,
            token_mint_a,
            token_mint_b,
            maker_token_account_b,
            vault,
            receive,
            bump,
        ),
        owner: crate::ID,
        executable: false,
    }
}

/// Mark specific account indices as signers on an instruction.
fn with_signers(mut ix: Instruction, indices: &[usize]) -> Instruction {
    for &i in indices {
        ix.accounts[i].is_signer = true;
    }
    ix
}

/// Build make_offer instruction data.
/// Wire format: [discriminator: u8 = 0] [id: u64 LE] [deposit: u64 LE] [receive: u64 LE]
fn build_make_offer_data(id: u64, deposit: u64, receive: u64) -> Vec<u8> {
    let mut data = vec![0u8];
    data.extend_from_slice(&id.to_le_bytes());
    data.extend_from_slice(&deposit.to_le_bytes());
    data.extend_from_slice(&receive.to_le_bytes());
    data
}

/// Build take_offer instruction data.
/// Wire format: [discriminator: u8 = 1]
fn build_take_offer_data() -> Vec<u8> {
    vec![1u8]
}

/// Build cancel_offer instruction data.
/// Wire format: [discriminator: u8 = 2]
fn build_cancel_offer_data() -> Vec<u8> {
    vec![2u8]
}

struct TakeOfferFixture {
    maker: Pubkey,
    taker: Pubkey,
    token_mint_a: Pubkey,
    token_mint_b: Pubkey,
    taker_token_account_a: Pubkey,
    taker_token_account_b: Pubkey,
    maker_token_account_b: Pubkey,
    vault: Pubkey,
    offer: Pubkey,
    offer_bump: u8,
}

fn take_offer_fixture() -> TakeOfferFixture {
    let maker = Pubkey::new_unique();
    let (offer, offer_bump) = derive_offer(&maker, OFFER_ID);
    TakeOfferFixture {
        maker,
        taker: Pubkey::new_unique(),
        token_mint_a: Pubkey::new_unique(),
        token_mint_b: Pubkey::new_unique(),
        taker_token_account_a: Pubkey::new_unique(),
        taker_token_account_b: Pubkey::new_unique(),
        maker_token_account_b: Pubkey::new_unique(),
        vault: Pubkey::new_unique(),
        offer,
        offer_bump,
    }
}

/// Build the take_offer instruction for the fixture, allowing the mint A and
/// vault metas to be overridden so attacks with substituted accounts can be
/// expressed.
fn build_take_offer_instruction(
    fx: &TakeOfferFixture,
    token_mint_a: Pubkey,
    vault: Pubkey,
) -> Instruction {
    let rent = quasar_svm::solana_sdk_ids::sysvar::rent::ID;
    with_signers(
        Instruction {
            program_id: crate::ID,
            accounts: vec![
                solana_instruction::AccountMeta::new(fx.taker.into(), true),
                solana_instruction::AccountMeta::new(fx.offer.into(), false),
                solana_instruction::AccountMeta::new(fx.maker.into(), false),
                solana_instruction::AccountMeta::new_readonly(token_mint_a.into(), false),
                solana_instruction::AccountMeta::new_readonly(fx.token_mint_b.into(), false),
                solana_instruction::AccountMeta::new(fx.taker_token_account_a.into(), false),
                solana_instruction::AccountMeta::new(fx.taker_token_account_b.into(), false),
                solana_instruction::AccountMeta::new(fx.maker_token_account_b.into(), false),
                solana_instruction::AccountMeta::new(vault.into(), false),
                solana_instruction::AccountMeta::new_readonly(rent.into(), false),
                solana_instruction::AccountMeta::new_readonly(
                    quasar_svm::SPL_TOKEN_PROGRAM_ID.into(),
                    false,
                ),
                solana_instruction::AccountMeta::new_readonly(
                    quasar_svm::system_program::ID.into(),
                    false,
                ),
            ],
            data: build_take_offer_data(),
        },
        // taker_token_account_a signs the create_account CPI for its own
        // initialization.
        &[5],
    )
}

fn take_offer_fixture_accounts(fx: &TakeOfferFixture) -> Vec<Account> {
    vec![
        signer(fx.taker),
        offer_account(
            fx.offer,
            OFFER_ID,
            fx.maker,
            fx.token_mint_a,
            fx.token_mint_b,
            fx.maker_token_account_b,
            fx.vault,
            RECEIVE_AMOUNT,
            fx.offer_bump,
        ),
        signer(fx.maker),
        mint(fx.token_mint_a, fx.maker),
        mint(fx.token_mint_b, fx.maker),
        empty(fx.taker_token_account_a),
        token(fx.taker_token_account_b, fx.token_mint_b, fx.taker, 10_000),
        token(fx.maker_token_account_b, fx.token_mint_b, fx.maker, 0),
        token(fx.vault, fx.token_mint_a, fx.offer, DEPOSIT_AMOUNT),
    ]
}

#[test]
fn test_make_offer() {
    let mut svm = setup();

    let token_program = quasar_svm::SPL_TOKEN_PROGRAM_ID;
    let system_program = quasar_svm::system_program::ID;
    let maker = Pubkey::new_unique();
    let token_mint_a = Pubkey::new_unique();
    let token_mint_b = Pubkey::new_unique();
    let maker_token_account_a = Pubkey::new_unique();
    let maker_token_account_b = Pubkey::new_unique();
    let vault = Pubkey::new_unique();
    let (offer, offer_bump) = derive_offer(&maker, OFFER_ID);
    let rent = quasar_svm::solana_sdk_ids::sysvar::rent::ID;

    let data = build_make_offer_data(OFFER_ID, DEPOSIT_AMOUNT, RECEIVE_AMOUNT);

    let instruction = with_signers(
        Instruction {
            program_id: crate::ID,
            accounts: vec![
                solana_instruction::AccountMeta::new(maker.into(), true),
                solana_instruction::AccountMeta::new(offer.into(), false),
                solana_instruction::AccountMeta::new_readonly(token_mint_a.into(), false),
                solana_instruction::AccountMeta::new_readonly(token_mint_b.into(), false),
                solana_instruction::AccountMeta::new(maker_token_account_a.into(), false),
                solana_instruction::AccountMeta::new(maker_token_account_b.into(), false),
                solana_instruction::AccountMeta::new(vault.into(), false),
                solana_instruction::AccountMeta::new_readonly(rent.into(), false),
                solana_instruction::AccountMeta::new_readonly(token_program.into(), false),
                solana_instruction::AccountMeta::new_readonly(system_program.into(), false),
            ],
            data,
        },
        &[5, 6], // maker_token_account_b, vault as signers for create_account CPI
    );

    let result = svm.process_instruction(
        &instruction,
        &[
            signer(maker),
            empty(offer),
            mint(token_mint_a, maker),
            mint(token_mint_b, maker),
            token(maker_token_account_a, token_mint_a, maker, 1_000_000),
            empty(maker_token_account_b),
            empty(vault),
        ],
    );

    assert!(result.is_ok(), "make_offer failed: {:?}", result.raw_result);

    // Verify offer state (layout documented on offer_data above).
    let offer_data = &result.account(&offer).unwrap().data;
    assert_eq!(offer_data[0], 1, "discriminator");
    assert_eq!(&offer_data[1..9], &OFFER_ID.to_le_bytes(), "id");
    assert_eq!(&offer_data[9..41], maker.as_ref(), "maker");
    assert_eq!(&offer_data[41..73], token_mint_a.as_ref(), "token_mint_a");
    assert_eq!(&offer_data[73..105], token_mint_b.as_ref(), "token_mint_b");
    assert_eq!(
        &offer_data[105..137],
        maker_token_account_b.as_ref(),
        "maker_token_account_b"
    );
    assert_eq!(&offer_data[137..169], vault.as_ref(), "vault");
    assert_eq!(
        &offer_data[169..177],
        &RECEIVE_AMOUNT.to_le_bytes(),
        "receive"
    );
    assert_eq!(offer_data[177], offer_bump, "bump");

    // The deposit landed in the vault.
    assert_eq!(
        token_amount(result.account(&vault).unwrap()),
        DEPOSIT_AMOUNT
    );

    println!("  MAKE_OFFER CU: {}", result.compute_units_consumed);
}

#[test]
fn test_take_offer() {
    let mut svm = setup();
    let fx = take_offer_fixture();
    let accounts = take_offer_fixture_accounts(&fx);
    let vault_rent = accounts[8].lamports;

    let instruction = build_take_offer_instruction(&fx, fx.token_mint_a, fx.vault);
    let result = svm.process_instruction(&instruction, &accounts);
    assert!(result.is_ok(), "take_offer failed: {:?}", result.raw_result);

    // Token balances: the taker received the vault's mint A, the maker
    // received the wanted mint B.
    assert_eq!(
        token_amount(result.account(&fx.taker_token_account_a).unwrap()),
        DEPOSIT_AMOUNT
    );
    assert_eq!(
        token_amount(result.account(&fx.maker_token_account_b).unwrap()),
        RECEIVE_AMOUNT
    );

    // The offer and vault are closed.
    let offer_lamports = result.account(&fx.offer).map(|a| a.lamports).unwrap_or(0);
    let vault_lamports = result.account(&fx.vault).map(|a| a.lamports).unwrap_or(0);
    assert_eq!(offer_lamports, 0, "offer must be closed");
    assert_eq!(vault_lamports, 0, "vault must be closed");

    // Rent destinations: the maker recovers the rent of both accounts they
    // paid for in make_offer; the taker gains no lamports from the close.
    let maker_lamports = result.account(&fx.maker).unwrap().lamports;
    let expected_maker_lamports = STARTING_LAMPORTS
        .checked_add(OFFER_ACCOUNT_LAMPORTS)
        .and_then(|lamports| lamports.checked_add(vault_rent))
        .unwrap();
    assert_eq!(
        maker_lamports, expected_maker_lamports,
        "maker must recover the offer and vault rent"
    );
    let taker_lamports = result.account(&fx.taker).unwrap().lamports;
    assert!(
        taker_lamports <= STARTING_LAMPORTS,
        "taker must not gain lamports from closing the maker's accounts"
    );

    println!("  TAKE_OFFER CU: {}", result.compute_units_consumed);
}

#[test]
fn test_take_offer_rejects_wrong_mint() {
    let mut svm = setup();
    let fx = take_offer_fixture();
    let mut accounts = take_offer_fixture_accounts(&fx);

    // The attacker substitutes a different mint for token_mint_a. The
    // has_one(token_mint_a) binding to the offer state must reject it.
    let wrong_mint = Pubkey::new_unique();
    accounts[3] = mint(wrong_mint, fx.maker);

    let instruction = build_take_offer_instruction(&fx, wrong_mint, fx.vault);
    let result = svm.process_instruction(&instruction, &accounts);
    assert!(
        !result.is_ok(),
        "take_offer must reject a mint that does not match the offer state"
    );
}

#[test]
fn test_take_offer_rejects_wrong_vault() {
    let mut svm = setup();
    let fx = take_offer_fixture();
    let mut accounts = take_offer_fixture_accounts(&fx);

    // The attacker substitutes a different token account (same mint, also
    // owned by the offer PDA) for the vault. The has_one(vault) binding to
    // the offer state must reject it.
    let wrong_vault = Pubkey::new_unique();
    accounts[8] = token(wrong_vault, fx.token_mint_a, fx.offer, DEPOSIT_AMOUNT);

    let instruction = build_take_offer_instruction(&fx, fx.token_mint_a, wrong_vault);
    let result = svm.process_instruction(&instruction, &accounts);
    assert!(
        !result.is_ok(),
        "take_offer must reject a vault that does not match the offer state"
    );
}

#[test]
fn test_cancel_offer() {
    let mut svm = setup();

    let maker = Pubkey::new_unique();
    let token_mint_a = Pubkey::new_unique();
    let token_mint_b = Pubkey::new_unique();
    let maker_token_account_a = Pubkey::new_unique();
    let maker_token_account_b = Pubkey::new_unique();
    let vault = Pubkey::new_unique();
    let (offer, offer_bump) = derive_offer(&maker, OFFER_ID);
    let rent = quasar_svm::solana_sdk_ids::sysvar::rent::ID;

    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            solana_instruction::AccountMeta::new(maker.into(), true),
            solana_instruction::AccountMeta::new(offer.into(), false),
            solana_instruction::AccountMeta::new_readonly(token_mint_a.into(), false),
            solana_instruction::AccountMeta::new(maker_token_account_a.into(), false),
            solana_instruction::AccountMeta::new(vault.into(), false),
            solana_instruction::AccountMeta::new_readonly(rent.into(), false),
            solana_instruction::AccountMeta::new_readonly(
                quasar_svm::SPL_TOKEN_PROGRAM_ID.into(),
                false,
            ),
            solana_instruction::AccountMeta::new_readonly(
                quasar_svm::system_program::ID.into(),
                false,
            ),
        ],
        data: build_cancel_offer_data(),
    };

    let vault_account = token(vault, token_mint_a, offer, DEPOSIT_AMOUNT);
    let vault_rent = vault_account.lamports;
    let result = svm.process_instruction(
        &instruction,
        &[
            signer(maker),
            offer_account(
                offer,
                OFFER_ID,
                maker,
                token_mint_a,
                token_mint_b,
                maker_token_account_b,
                vault,
                RECEIVE_AMOUNT,
                offer_bump,
            ),
            mint(token_mint_a, maker),
            // Pre-created with a zero balance so the maker's lamports can be
            // compared exactly after the cancel.
            token(maker_token_account_a, token_mint_a, maker, 0),
            vault_account,
        ],
    );

    assert!(
        result.is_ok(),
        "cancel_offer failed: {:?}",
        result.raw_result
    );

    // The maker got their mint A tokens back.
    assert_eq!(
        token_amount(result.account(&maker_token_account_a).unwrap()),
        DEPOSIT_AMOUNT
    );

    // The offer and vault are closed and their rent returned to the maker.
    let offer_lamports = result.account(&offer).map(|a| a.lamports).unwrap_or(0);
    let vault_lamports = result.account(&vault).map(|a| a.lamports).unwrap_or(0);
    assert_eq!(offer_lamports, 0, "offer must be closed");
    assert_eq!(vault_lamports, 0, "vault must be closed");

    let maker_lamports = result.account(&maker).unwrap().lamports;
    let expected_maker_lamports = STARTING_LAMPORTS
        .checked_add(OFFER_ACCOUNT_LAMPORTS)
        .and_then(|lamports| lamports.checked_add(vault_rent))
        .unwrap();
    assert_eq!(
        maker_lamports, expected_maker_lamports,
        "maker must recover the offer and vault rent"
    );

    println!("  CANCEL_OFFER CU: {}", result.compute_units_consumed);
}

#[test]
fn test_cancel_offer_rejects_non_maker() {
    let mut svm = setup();

    let maker = Pubkey::new_unique();
    let attacker = Pubkey::new_unique();
    let token_mint_a = Pubkey::new_unique();
    let token_mint_b = Pubkey::new_unique();
    let attacker_token_account_a = Pubkey::new_unique();
    let maker_token_account_b = Pubkey::new_unique();
    let vault = Pubkey::new_unique();
    let (offer, offer_bump) = derive_offer(&maker, OFFER_ID);
    let rent = quasar_svm::solana_sdk_ids::sysvar::rent::ID;

    // The attacker signs as the "maker". has_one(maker) and the offer's PDA
    // seeds both fail to match.
    let instruction = Instruction {
        program_id: crate::ID,
        accounts: vec![
            solana_instruction::AccountMeta::new(attacker.into(), true),
            solana_instruction::AccountMeta::new(offer.into(), false),
            solana_instruction::AccountMeta::new_readonly(token_mint_a.into(), false),
            solana_instruction::AccountMeta::new(attacker_token_account_a.into(), false),
            solana_instruction::AccountMeta::new(vault.into(), false),
            solana_instruction::AccountMeta::new_readonly(rent.into(), false),
            solana_instruction::AccountMeta::new_readonly(
                quasar_svm::SPL_TOKEN_PROGRAM_ID.into(),
                false,
            ),
            solana_instruction::AccountMeta::new_readonly(
                quasar_svm::system_program::ID.into(),
                false,
            ),
        ],
        data: build_cancel_offer_data(),
    };

    let result = svm.process_instruction(
        &instruction,
        &[
            signer(attacker),
            offer_account(
                offer,
                OFFER_ID,
                maker,
                token_mint_a,
                token_mint_b,
                maker_token_account_b,
                vault,
                RECEIVE_AMOUNT,
                offer_bump,
            ),
            mint(token_mint_a, maker),
            token(attacker_token_account_a, token_mint_a, attacker, 0),
            token(vault, token_mint_a, offer, DEPOSIT_AMOUNT),
        ],
    );

    assert!(
        !result.is_ok(),
        "cancel_offer must reject a signer who is not the offer's maker"
    );
}
