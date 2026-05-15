// World setup: spin up a LiteSVM with our hackathon program + Squads loaded,
// fund the committee, create a USDC-style mint, and bring up a 2-of-3 Squads
// multisig (Alice / Bob / Carol). Returns a `World` that the per-test files
// use as their starting state.

use anchor_lang::prelude::Pubkey;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::{InstructionData, ToAccountMetas};
use anchor_spl::associated_token::get_associated_token_address;
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_kite::{
    create_associated_token_account, create_token_mint, create_wallet,
    mint_tokens_to_token_account, send_transaction_from_instructions,
};
use solana_signer::Signer;

use super::squads::{
    forge_program_config, install_squads_program, multisig_create_v2_instruction, multisig_pda,
    proposal_approve_instruction, proposal_create_instruction, serialize_transaction_message,
    vault_pda, vault_transaction_create_instruction, vault_transaction_execute_instruction,
    CompiledInstruction, Committee,
};


// USDC has 6 decimals, so we mirror that here for a realistic prize-amount
// feel in tests.
pub const PRIZE_MINT_DECIMALS: u8 = 6;
pub const ONE_TOKEN: u64 = 10u64.pow(PRIZE_MINT_DECIMALS as u32);

pub struct World {
    pub svm: LiteSVM,
    pub payer: Keypair,
    pub committee: Committee,
    pub mint: Pubkey,
    pub mint_authority: Keypair,
    // Treasury keypair used as the Squads program-config treasury. With a
    // creation fee of 0, it never receives lamports, but it must still be a
    // System-owned account so the SystemProgram transfer accounts validate.
    pub treasury: Keypair,
}

pub fn setup_world() -> World {
    let mut svm = LiteSVM::new();

    // Load our hackathon program. The `.so` is built by `cargo build-sbf` —
    // the test harness expects it to exist at the canonical anchor target
    // path.
    let hackathon_so = include_bytes!("../../../../target/deploy/hackathon.so");
    svm.add_program(hackathon::id(), hackathon_so).unwrap();

    // Load Squads v4 from the vendored fixture.
    install_squads_program(&mut svm);

    let payer = create_wallet(&mut svm, 100_000_000_000).unwrap();

    // Treasury account: a fresh system-owned wallet. Funded so it stays
    // rent-exempt as a SystemAccount even though the creation fee is 0.
    let treasury = create_wallet(&mut svm, 1_000_000_000).unwrap();
    forge_program_config(&mut svm, &treasury.pubkey());

    // Committee. Each member is independently funded so they can pay
    // transaction fees when voting.
    let alice = create_wallet(&mut svm, 10_000_000_000).unwrap();
    let bob = create_wallet(&mut svm, 10_000_000_000).unwrap();
    let carol = create_wallet(&mut svm, 10_000_000_000).unwrap();

    let create_key = Keypair::new();
    let (multisig, _bump) = multisig_pda(&create_key.pubkey());
    let (vault, vault_bump) = vault_pda(&multisig, 0);

    let committee = Committee {
        alice,
        bob,
        carol,
        create_key,
        multisig,
        vault,
        vault_bump,
    };

    // Create the multisig (threshold 2-of-3, all members hold every
    // permission). Payer funds the multisig account rent.
    let create_ix = multisig_create_v2_instruction(
        &committee.create_key.pubkey(),
        &payer.pubkey(),
        &treasury.pubkey(),
        committee.members_sorted(),
        2,
    );
    send_transaction_from_instructions(
        &mut svm,
        vec![create_ix],
        &[&payer, &committee.create_key],
        &payer.pubkey(),
    )
    .expect("multisig_create_v2 succeeded");

    // Vault PDA needs lamports to fund downstream ATAs and to sign CPIs
    // (rent is paid from the executing member, but the vault itself must
    // be rent-exempt for any SystemProgram interactions).
    svm.airdrop(&committee.vault, 5_000_000_000).unwrap();

    // Prize-denominating mint. Mint authority is a throwaway keypair held
    // by the test, not the committee. solana_kite::create_token_mint takes
    // the mint authority as a Keypair and returns the mint Pubkey directly;
    // the authority also pays the initialize-mint transaction fee, so it has
    // to be funded first.
    let mint_authority = create_wallet(&mut svm, 1_000_000_000).unwrap();
    let mint = create_token_mint(&mut svm, &mint_authority, PRIZE_MINT_DECIMALS, None)
        .expect("mint created");

    World {
        svm,
        payer,
        committee,
        mint,
        mint_authority,
        treasury,
    }
}

// Run a full Squads multisig flow against a single instruction signed by the
// vault: create vault transaction, create proposal, approve with two members
// (Alice and Bob — Carol's vote is unnecessary at threshold 2), execute.
//
// `inner` is the instruction the vault should sign. Its `AccountMeta.pubkey`
// list is taken verbatim as the message account_keys; `is_signer` /
// `is_writable` flags are used both to encode the compiled message and to
// build the `remaining_accounts` vector passed to `vault_transaction_execute`.
pub fn run_through_multisig(world: &mut World, inner: Instruction) -> u64 {
    let multisig_account = world.svm.get_account(&world.committee.multisig).unwrap();
    let current_index = u64::from_le_bytes(
        multisig_account.data[8 + 32 + 32 + 2 + 4..8 + 32 + 32 + 2 + 4 + 8]
            .try_into()
            .unwrap(),
    );
    let next_index = current_index + 1;

    // ----- Compile the inner instruction into Squads' message format -----
    //
    // Layout we need:
    //   account_keys = [vault, ...other_accounts..., inner.program_id]
    //
    // The vault must be at index 0 and is the (only) signer in this
    // message. Squads validates that account_keys[0..num_signers] are signed
    // by the vault PDA at execute time.

    let vault = world.committee.vault;
    let inner_program_id = inner.program_id;

    // Collect unique accounts referenced by the inner instruction, excluding
    // the vault (which we always place at index 0) and the inner program
    // (which we always append last). Preserve first-seen order.
    let mut account_keys: Vec<Pubkey> = vec![vault];
    let mut is_writable: Vec<bool> = vec![true]; // vault is always treated as writable in our tests
    let mut is_signer_flags: Vec<bool> = vec![true]; // vault signs (via PDA)

    for meta in &inner.accounts {
        if meta.pubkey == vault {
            // Already inserted at index 0; merge writable flag conservatively.
            if meta.is_writable {
                is_writable[0] = true;
            }
            continue;
        }
        if let Some(existing) = account_keys.iter().position(|k| *k == meta.pubkey) {
            if meta.is_writable {
                is_writable[existing] = true;
            }
            // Inner instructions never have non-vault signers in our tests.
        } else {
            account_keys.push(meta.pubkey);
            is_writable.push(meta.is_writable);
            is_signer_flags.push(false);
        }
    }

    // Append program id. Programs are never signers and never writable.
    let program_index = account_keys.len() as u8;
    account_keys.push(inner_program_id);
    is_writable.push(false);
    is_signer_flags.push(false);

    // Squads message header counts. Layout convention:
    //   keys[0..num_signers] = signers
    //   of which [0..num_writable_signers] are writable
    //   keys[num_signers..num_signers+num_writable_non_signers] = writable non-signers
    //   rest = readonly non-signers
    //
    // Reorder so the layout matches that contract.
    let num_signers = is_signer_flags.iter().filter(|s| **s).count() as u8;
    debug_assert!(num_signers >= 1, "vault must sign");

    // After our construction above, only `vault` is a signer and it sits at
    // index 0. Verify and split signers/non-signers without further
    // shuffling: signers = [0], non-signers = [1..].
    debug_assert!(is_signer_flags[0], "vault index 0 must be signer");
    for flag in is_signer_flags.iter().skip(1) {
        debug_assert!(!*flag, "no other signers expected in inner ix");
    }

    let num_writable_signers = if is_writable[0] { 1 } else { 0 };

    // Reorder non-signers so writable ones come first.
    let mut writable_non_signers: Vec<(Pubkey, usize)> = Vec::new();
    let mut readonly_non_signers: Vec<(Pubkey, usize)> = Vec::new();
    for (i, key) in account_keys.iter().enumerate().skip(1) {
        if is_writable[i] {
            writable_non_signers.push((*key, i));
        } else {
            readonly_non_signers.push((*key, i));
        }
    }
    let num_writable_non_signers = writable_non_signers.len() as u8;

    // Rebuild `account_keys` and an index-remapping table.
    let mut remap = vec![0u8; account_keys.len()];
    let mut new_keys: Vec<Pubkey> = Vec::with_capacity(account_keys.len());
    new_keys.push(account_keys[0]); // vault
    remap[0] = 0;
    let mut next_slot: u8 = 1;
    for (key, old_index) in &writable_non_signers {
        new_keys.push(*key);
        remap[*old_index] = next_slot;
        next_slot += 1;
    }
    for (key, old_index) in &readonly_non_signers {
        new_keys.push(*key);
        remap[*old_index] = next_slot;
        next_slot += 1;
    }
    // Program id: find its old index and remap.
    let old_program_index = program_index as usize;
    let new_program_index = remap[old_program_index];

    let compiled_account_indexes: Vec<u8> = inner
        .accounts
        .iter()
        .map(|meta| {
            if meta.pubkey == vault {
                0
            } else {
                let old = account_keys.iter().position(|k| *k == meta.pubkey).unwrap();
                remap[old]
            }
        })
        .collect();

    let compiled = CompiledInstruction {
        program_id_index: new_program_index,
        account_indexes: compiled_account_indexes,
        data: inner.data.clone(),
    };

    let message = serialize_transaction_message(
        num_signers,
        num_writable_signers,
        num_writable_non_signers,
        &new_keys,
        &[compiled],
    );

    // ----- 1. vault_transaction_create -----
    let create_tx_ix = vault_transaction_create_instruction(
        &world.committee.multisig,
        next_index,
        0, // vault_index
        &world.committee.alice.pubkey(),
        &world.payer.pubkey(),
        message,
    );

    // ----- 2. proposal_create -----
    let create_proposal_ix = proposal_create_instruction(
        &world.committee.multisig,
        next_index,
        &world.committee.alice.pubkey(),
        &world.payer.pubkey(),
    );

    // ----- 3. proposal_approve x2 (Alice + Bob, threshold = 2) -----
    let approve_alice_ix = proposal_approve_instruction(
        &world.committee.multisig,
        next_index,
        &world.committee.alice.pubkey(),
    );
    let approve_bob_ix = proposal_approve_instruction(
        &world.committee.multisig,
        next_index,
        &world.committee.bob.pubkey(),
    );

    // ----- 4. vault_transaction_execute -----
    //
    // remaining_accounts must follow new_keys order, with writable / signer
    // flags matching the compiled-message contract. Squads sets the vault's
    // signer bit via its own PDA signing — we must NOT mark it as a signer
    // in the outer transaction.
    let remaining_accounts: Vec<AccountMeta> = new_keys
        .iter()
        .enumerate()
        .map(|(i, key)| {
            let writable = if i == 0 {
                is_writable[0]
            } else {
                let old = account_keys.iter().position(|k| k == key).unwrap();
                is_writable[old]
            };
            AccountMeta {
                pubkey: *key,
                is_signer: false,
                is_writable: writable,
            }
        })
        .collect();

    let execute_ix = vault_transaction_execute_instruction(
        &world.committee.multisig,
        next_index,
        &world.committee.alice.pubkey(),
        remaining_accounts,
    );

    send_transaction_from_instructions(
        &mut world.svm,
        vec![create_tx_ix, create_proposal_ix],
        &[&world.payer, &world.committee.alice],
        &world.payer.pubkey(),
    )
    .expect("create vault transaction + proposal");

    send_transaction_from_instructions(
        &mut world.svm,
        vec![approve_alice_ix],
        &[&world.committee.alice],
        &world.committee.alice.pubkey(),
    )
    .expect("alice approves");

    send_transaction_from_instructions(
        &mut world.svm,
        vec![approve_bob_ix],
        &[&world.committee.bob],
        &world.committee.bob.pubkey(),
    )
    .expect("bob approves");

    send_transaction_from_instructions(
        &mut world.svm,
        vec![execute_ix],
        &[&world.committee.alice],
        &world.committee.alice.pubkey(),
    )
    .expect("vault_transaction_execute");

    next_index
}

// ----- Hackathon program instruction builders --------------------------
//
// Built via Anchor's generated `accounts::*` and `instruction::*` modules so
// we get compile-time wire-format correctness for our own program. We only
// hand-roll the wire format for Squads.

pub fn create_hackathon_instruction(
    payer: &Pubkey,
    authority: &Pubkey,
    hackathon: &Pubkey,
    name: String,
) -> Instruction {
    Instruction {
        program_id: hackathon::id(),
        accounts: hackathon::accounts::CreateHackathon {
            payer: *payer,
            authority: *authority,
            hackathon: *hackathon,
            system_program: anchor_lang::solana_program::system_program::ID,
        }
        .to_account_metas(None),
        data: hackathon::instruction::CreateHackathon { name }.data(),
    }
}

pub fn add_prize_instruction(
    payer: &Pubkey,
    authority: &Pubkey,
    hackathon: &Pubkey,
    mint: &Pubkey,
    prize: &Pubkey,
    vault: &Pubkey,
    amount: u64,
) -> Instruction {
    Instruction {
        program_id: hackathon::id(),
        accounts: hackathon::accounts::AddPrize {
            payer: *payer,
            authority: *authority,
            hackathon: *hackathon,
            mint: *mint,
            prize: *prize,
            vault: *vault,
            associated_token_program: anchor_spl::associated_token::ID,
            token_program: anchor_spl::token::ID,
            system_program: anchor_lang::solana_program::system_program::ID,
        }
        .to_account_metas(None),
        data: hackathon::instruction::AddPrize { amount }.data(),
    }
}

pub fn set_winner_instruction(
    authority: &Pubkey,
    hackathon: &Pubkey,
    prize: &Pubkey,
    prize_index: u8,
    winner: Pubkey,
) -> Instruction {
    Instruction {
        program_id: hackathon::id(),
        accounts: hackathon::accounts::SetWinner {
            authority: *authority,
            hackathon: *hackathon,
            prize: *prize,
        }
        .to_account_metas(None),
        data: hackathon::instruction::SetWinner {
            prize_index,
            winner,
        }
        .data(),
    }
}

pub fn pay_winner_instruction(
    caller: &Pubkey,
    hackathon: &Pubkey,
    prize: &Pubkey,
    mint: &Pubkey,
    vault: &Pubkey,
    winner_token_account: &Pubkey,
    prize_index: u8,
) -> Instruction {
    Instruction {
        program_id: hackathon::id(),
        accounts: hackathon::accounts::PayWinner {
            caller: *caller,
            hackathon: *hackathon,
            prize: *prize,
            mint: *mint,
            vault: *vault,
            winner_token_account: *winner_token_account,
            token_program: anchor_spl::token::ID,
        }
        .to_account_metas(None),
        data: hackathon::instruction::PayWinner { prize_index }.data(),
    }
}

pub fn cancel_prize_instruction(
    authority: &Pubkey,
    rent_destination: &Pubkey,
    hackathon: &Pubkey,
    prize: &Pubkey,
    mint: &Pubkey,
    vault: &Pubkey,
    refund_token_account: &Pubkey,
    prize_index: u8,
) -> Instruction {
    Instruction {
        program_id: hackathon::id(),
        accounts: hackathon::accounts::CancelPrize {
            authority: *authority,
            rent_destination: *rent_destination,
            hackathon: *hackathon,
            prize: *prize,
            mint: *mint,
            vault: *vault,
            refund_token_account: *refund_token_account,
            token_program: anchor_spl::token::ID,
        }
        .to_account_metas(None),
        data: hackathon::instruction::CancelPrize { prize_index }.data(),
    }
}

pub fn close_hackathon_instruction(
    authority: &Pubkey,
    rent_destination: &Pubkey,
    hackathon: &Pubkey,
    prize_accounts: &[Pubkey],
) -> Instruction {
    let mut accounts = hackathon::accounts::CloseHackathon {
        authority: *authority,
        rent_destination: *rent_destination,
        hackathon: *hackathon,
    }
    .to_account_metas(None);
    for prize in prize_accounts {
        accounts.push(AccountMeta::new(*prize, false));
    }
    Instruction {
        program_id: hackathon::id(),
        accounts,
        data: hackathon::instruction::CloseHackathon {}.data(),
    }
}

// ----- PDA helpers for hackathon accounts ------------------------------

pub fn hackathon_pda(authority: &Pubkey, name: &str) -> (Pubkey, u8) {
    let name_seed = hackathon_name_seed(name);
    Pubkey::find_program_address(
        &[b"hackathon", authority.as_ref(), name_seed.as_ref()],
        &hackathon::id(),
    )
}

pub fn prize_pda(hackathon_account: &Pubkey, index: u8) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[b"prize", hackathon_account.as_ref(), &[index]],
        &hackathon::id(),
    )
}

pub fn prize_vault_address(prize: &Pubkey, mint: &Pubkey) -> Pubkey {
    get_associated_token_address(prize, mint)
}

// Local mirror of the program's `name_seed` so tests can derive the PDA
// without depending on the program's private `instructions::shared` module.
fn hackathon_name_seed(name: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(name.as_bytes());
    hasher.finalize().into()
}

// Mint `amount` tokens to the prize's vault ATA. Used by tests that need a
// funded prize. solana_kite arg order: (svm, mint, token_account, amount,
// mint_authority).
//
// We expire the blockhash before each call so identical `fund_prize_vault`
// invocations within one test (e.g. fund → consume → fund again) produce
// unique signatures and avoid LiteSVM's `AlreadyProcessed` dedup.
pub fn fund_prize_vault(world: &mut World, vault: &Pubkey, amount: u64) {
    world.svm.expire_blockhash();
    mint_tokens_to_token_account(
        &mut world.svm,
        &world.mint,
        vault,
        amount,
        &world.mint_authority,
    )
    .expect("fund prize vault");
}

// Create a token account owned by `owner` for the world's mint. solana_kite
// arg order: (svm, owner, mint, payer).
pub fn create_token_account_for(world: &mut World, owner: &Pubkey) -> Pubkey {
    create_associated_token_account(&mut world.svm, owner, &world.mint, &world.payer)
        .expect("create token account")
}
