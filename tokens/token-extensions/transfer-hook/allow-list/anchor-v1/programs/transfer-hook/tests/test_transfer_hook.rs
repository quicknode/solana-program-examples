use {
    anchor_lang::{
        solana_program::{instruction::Instruction, pubkey::Pubkey, system_program},
        InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_kite::{
        create_wallet, send_transaction_from_instructions,
        token_extensions::{
            create_token_extensions_account, create_token_extensions_mint,
            mint_tokens_to_token_extensions_account, transfer_checked_token_extensions,
            MintExtension,
        },
        transfer_hook::{build_hook_accounts, get_hook_accounts_address, HookAccount},
        SolanaKiteError,
    },
    solana_signer::Signer,
    transfer_hook::{ALLOWED_WALLET_SEED, ALLOW_LIST_SEED},
};

const DECIMALS: u8 = 9;
const ONE_TOKEN: u64 = 10u64.pow(DECIMALS as u32);

// Anchor numbers custom errors from 6000 in declaration order.
const RECIPIENT_NOT_ALLOWED: &str = "Custom(6001)";

/// A mint with the transfer hook attached, its allow list set up, and a
/// sender holding 100 tokens. `issuer` is the mint authority and so the allow
/// list's authority.
struct Fixture {
    svm: LiteSVM,
    program_id: Pubkey,
    issuer: Keypair,
    mint: Pubkey,
    allow_list: Pubkey,
    sender_token: Pubkey,
}

fn allowed_wallet_address(program_id: &Pubkey, mint: &Pubkey, wallet: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[ALLOWED_WALLET_SEED, mint.as_ref(), wallet.as_ref()],
        program_id,
    )
    .0
}

fn create_hooked_mint(svm: &mut LiteSVM, program_id: Pubkey, issuer: &Keypair) -> Pubkey {
    let mint = create_token_extensions_mint(
        svm,
        issuer,
        DECIMALS,
        None,
        &[MintExtension::TransferHook { program_id }],
        None,
    )
    .unwrap();
    svm.expire_blockhash();
    mint
}

fn initialize_instruction(program_id: Pubkey, payer: &Pubkey, mint: &Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        program_id,
        &transfer_hook::instruction::InitializeExtraAccountMetaList {}.data(),
        transfer_hook::accounts::InitializeExtraAccountMetaListAccountConstraints {
            payer: *payer,
            extra_account_meta_list: get_hook_accounts_address(mint, &program_id),
            mint: *mint,
            allow_list: Pubkey::find_program_address(
                &[ALLOW_LIST_SEED, mint.as_ref()],
                &program_id,
            )
            .0,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn setup() -> Fixture {
    let program_id = transfer_hook::id();
    let mut svm = LiteSVM::new();
    let program_bytes = include_bytes!("../../../target/deploy/transfer_hook.so");
    svm.add_program(program_id, program_bytes).unwrap();

    let issuer = create_wallet(&mut svm, 10_000_000_000).unwrap();
    let mint = create_hooked_mint(&mut svm, program_id, &issuer);
    let allow_list = Pubkey::find_program_address(&[ALLOW_LIST_SEED, mint.as_ref()], &program_id).0;

    send(
        &mut svm,
        initialize_instruction(program_id, &issuer.pubkey(), &mint),
        &issuer,
    )
    .unwrap();

    let sender_token =
        create_token_extensions_account(&mut svm, &issuer.pubkey(), &mint, &issuer, None).unwrap();
    svm.expire_blockhash();
    mint_tokens_to_token_extensions_account(
        &mut svm,
        &mint,
        &sender_token,
        100 * ONE_TOKEN,
        &issuer,
        None,
    )
    .unwrap();
    svm.expire_blockhash();

    Fixture {
        svm,
        program_id,
        issuer,
        mint,
        allow_list,
        sender_token,
    }
}

fn send(
    svm: &mut LiteSVM,
    instruction: Instruction,
    signer: &Keypair,
) -> Result<(), SolanaKiteError> {
    let result = send_transaction_from_instructions(
        svm,
        vec![instruction],
        &[signer],
        &signer.pubkey(),
        None,
    );
    svm.expire_blockhash();
    result
}

impl Fixture {
    fn allow_wallet_instruction(&self, authority: &Pubkey, wallet: &Pubkey) -> Instruction {
        Instruction::new_with_bytes(
            self.program_id,
            &transfer_hook::instruction::AllowWallet {}.data(),
            transfer_hook::accounts::AllowWalletAccountConstraints {
                authority: *authority,
                mint: self.mint,
                allow_list: self.allow_list,
                wallet: *wallet,
                allowed_wallet: allowed_wallet_address(&self.program_id, &self.mint, wallet),
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        )
    }

    fn allow_wallet(&mut self, wallet: &Pubkey) {
        let instruction = self.allow_wallet_instruction(&self.issuer.pubkey(), wallet);
        send(&mut self.svm, instruction, &self.issuer).unwrap();
    }

    fn disallow_wallet(&mut self, wallet: &Pubkey) {
        let instruction = Instruction::new_with_bytes(
            self.program_id,
            &transfer_hook::instruction::DisallowWallet {}.data(),
            transfer_hook::accounts::DisallowWalletAccountConstraints {
                authority: self.issuer.pubkey(),
                mint: self.mint,
                allow_list: self.allow_list,
                wallet: *wallet,
                allowed_wallet: allowed_wallet_address(&self.program_id, &self.mint, wallet),
            }
            .to_account_metas(None),
        );
        send(&mut self.svm, instruction, &self.issuer).unwrap();
    }

    /// A token account for a fresh wallet, returned with that wallet.
    fn new_recipient(&mut self) -> (Keypair, Pubkey) {
        let wallet = Keypair::new();
        let token_account = create_token_extensions_account(
            &mut self.svm,
            &wallet.pubkey(),
            &self.mint,
            &self.issuer,
            None,
        )
        .unwrap();
        self.svm.expire_blockhash();
        (wallet, token_account)
    }

    fn transfer_to(
        &mut self,
        wallet: &Pubkey,
        token_account: &Pubkey,
    ) -> Result<(), SolanaKiteError> {
        let extra_accounts = build_hook_accounts(
            &self.mint,
            &self.program_id,
            &[HookAccount {
                pubkey: allowed_wallet_address(&self.program_id, &self.mint, wallet),
                is_signer: false,
                is_writable: false,
            }],
        );
        let result = transfer_checked_token_extensions(
            &mut self.svm,
            &self.sender_token,
            &self.mint,
            token_account,
            &self.issuer,
            ONE_TOKEN,
            DECIMALS,
            &extra_accounts,
            None,
        );
        self.svm.expire_blockhash();
        result
    }
}

fn assert_rejected(result: Result<(), SolanaKiteError>, expected: &str) {
    let error = format!(
        "{:?}",
        result.expect_err("transfer should have been rejected")
    );
    assert!(error.contains(expected), "expected {expected}, got {error}");
}

#[test]
fn transfer_to_an_allowed_wallet_succeeds() {
    let mut fixture = setup();
    let (wallet, token_account) = fixture.new_recipient();
    fixture.allow_wallet(&wallet.pubkey());

    fixture
        .transfer_to(&wallet.pubkey(), &token_account)
        .unwrap();
}

#[test]
fn transfer_to_a_wallet_not_on_the_list_is_rejected() {
    let mut fixture = setup();
    let (wallet, token_account) = fixture.new_recipient();

    assert_rejected(
        fixture.transfer_to(&wallet.pubkey(), &token_account),
        RECIPIENT_NOT_ALLOWED,
    );
}

#[test]
fn disallowing_a_wallet_stops_transfers_to_it() {
    let mut fixture = setup();
    let (wallet, token_account) = fixture.new_recipient();
    fixture.allow_wallet(&wallet.pubkey());
    fixture
        .transfer_to(&wallet.pubkey(), &token_account)
        .unwrap();

    fixture.disallow_wallet(&wallet.pubkey());

    assert_rejected(
        fixture.transfer_to(&wallet.pubkey(), &token_account),
        RECIPIENT_NOT_ALLOWED,
    );
    let entry = allowed_wallet_address(&fixture.program_id, &fixture.mint, &wallet.pubkey());
    assert!(
        fixture
            .svm
            .get_account(&entry)
            .map_or(true, |account| account.lamports == 0),
        "disallow_wallet should close the AllowedWallet account"
    );
}

/// The list has no size limit: each allowed wallet is its own account, so the
/// fiftieth costs the same as the first. The single-account design this
/// example replaced held eleven.
#[test]
fn the_list_holds_more_wallets_than_one_account_could() {
    let mut fixture = setup();
    let mut recipients = Vec::new();
    for _ in 0..50 {
        let (wallet, token_account) = fixture.new_recipient();
        fixture.allow_wallet(&wallet.pubkey());
        recipients.push((wallet, token_account));
    }

    for (wallet, token_account) in &recipients {
        fixture
            .transfer_to(&wallet.pubkey(), token_account)
            .unwrap();
    }
}

/// Allowing a wallet for one mint says nothing about another mint.
#[test]
fn an_allowed_wallet_is_allowed_for_one_mint_only() {
    let mut fixture = setup();
    let (wallet, _) = fixture.new_recipient();
    fixture.allow_wallet(&wallet.pubkey());

    let other_mint = create_hooked_mint(&mut fixture.svm, fixture.program_id, &fixture.issuer);
    send(
        &mut fixture.svm,
        initialize_instruction(fixture.program_id, &fixture.issuer.pubkey(), &other_mint),
        &fixture.issuer,
    )
    .unwrap();
    let other_sender = create_token_extensions_account(
        &mut fixture.svm,
        &fixture.issuer.pubkey(),
        &other_mint,
        &fixture.issuer,
        None,
    )
    .unwrap();
    fixture.svm.expire_blockhash();
    mint_tokens_to_token_extensions_account(
        &mut fixture.svm,
        &other_mint,
        &other_sender,
        ONE_TOKEN,
        &fixture.issuer,
        None,
    )
    .unwrap();
    fixture.svm.expire_blockhash();
    let other_recipient = create_token_extensions_account(
        &mut fixture.svm,
        &wallet.pubkey(),
        &other_mint,
        &fixture.issuer,
        None,
    )
    .unwrap();
    fixture.svm.expire_blockhash();

    let extra_accounts = build_hook_accounts(
        &other_mint,
        &fixture.program_id,
        &[HookAccount {
            pubkey: allowed_wallet_address(&fixture.program_id, &other_mint, &wallet.pubkey()),
            is_signer: false,
            is_writable: false,
        }],
    );
    assert_rejected(
        transfer_checked_token_extensions(
            &mut fixture.svm,
            &other_sender,
            &other_mint,
            &other_recipient,
            &fixture.issuer,
            ONE_TOKEN,
            DECIMALS,
            &extra_accounts,
            None,
        ),
        RECIPIENT_NOT_ALLOWED,
    );
}

/// Only the mint authority can set up a mint's allow list, so nobody can
/// claim the list for a mint they do not issue.
#[test]
fn only_the_mint_authority_can_create_the_allow_list() {
    let program_id = transfer_hook::id();
    let mut svm = LiteSVM::new();
    let program_bytes = include_bytes!("../../../target/deploy/transfer_hook.so");
    svm.add_program(program_id, program_bytes).unwrap();
    let issuer = create_wallet(&mut svm, 10_000_000_000).unwrap();
    let attacker = create_wallet(&mut svm, 10_000_000_000).unwrap();
    let mint = create_hooked_mint(&mut svm, program_id, &issuer);

    let result = send(
        &mut svm,
        initialize_instruction(program_id, &attacker.pubkey(), &mint),
        &attacker,
    );
    assert!(result.is_err(), "a stranger must not create the allow list");

    send(
        &mut svm,
        initialize_instruction(program_id, &issuer.pubkey(), &mint),
        &issuer,
    )
    .unwrap();
}

#[test]
fn only_the_authority_can_allow_a_wallet() {
    let mut fixture = setup();
    let attacker = create_wallet(&mut fixture.svm, 10_000_000_000).unwrap();
    let (wallet, token_account) = fixture.new_recipient();

    let instruction = fixture.allow_wallet_instruction(&attacker.pubkey(), &wallet.pubkey());
    assert!(send(&mut fixture.svm, instruction, &attacker).is_err());

    assert_rejected(
        fixture.transfer_to(&wallet.pubkey(), &token_account),
        RECIPIENT_NOT_ALLOWED,
    );
}
