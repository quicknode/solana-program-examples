//! LiteSVM integration test for the `extension_nft` "chop tree" game program.
//!
//! It drives the full happy path against an in-memory validator:
//! 1. `init_player` - create the player + game-data PDAs.
//! 2. `mint_nft` - mint a Token Extensions NFT that carries its metadata inline via
//!    the metadata-pointer + token-metadata extensions.
//! 3. `chop_tree` - gain wood/lose energy and push the new wood total into the
//!    NFT metadata as an additional field.
//!
//! The session-keys lesson (`#[session_auth_or]`) is exercised through its
//! *fallback* branch: `chop_tree` is signed directly by the player's main
//! wallet with `session_token = None`, so the macro checks
//! `player.authority == signer`. This keeps the test self-contained - it does
//! not need the onchain session-keys program as a fixture, because the program
//! never CPIs into it (the session token is only ever read as an account).
//!
//! IMPORTANT: CI runs `anchor keys sync` before building, which rewrites the
//! program's `declare_id!`. We therefore reference the id via `extension_nft::ID`
//! (the crate constant) rather than a hardcoded literal, so the test keeps
//! working after the id is regenerated.

use {
    anchor_lang::{
        prelude::Pubkey, solana_program::system_program, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_instruction::Instruction,
    solana_keypair::Keypair,
    solana_kite::{create_wallet, get_pda_and_bump, send_transaction_from_instructions, Seed},
    solana_signer::Signer,
};

// Token Extensions and Associated-Token-Account program ids (the modern, fixed
// onchain addresses bundled by LiteSVM).
const TOKEN_2022_ID: Pubkey = Pubkey::from_str_const("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
const ASSOCIATED_TOKEN_ID: Pubkey =
    Pubkey::from_str_const("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
const RENT_SYSVAR_ID: Pubkey =
    Pubkey::from_str_const("SysvarRent111111111111111111111111111111111");

const LEVEL_SEED: &str = "level1";

fn setup() -> (LiteSVM, Pubkey) {
    let program_id = extension_nft::ID;
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/extension_nft.so");
    svm.add_program(program_id, bytes).unwrap();
    (svm, program_id)
}

/// Derive the player PDA: seeds = [b"player", authority].
fn player_pda(program_id: &Pubkey, authority: &Pubkey) -> Pubkey {
    get_pda_and_bump(
        &[Seed::from(b"player".as_ref()), Seed::from(*authority)],
        program_id,
    )
    .0
}

/// Derive the game-data PDA: seeds = [level_seed].
fn game_data_pda(program_id: &Pubkey, level_seed: &str) -> Pubkey {
    get_pda_and_bump(&[Seed::from(level_seed)], program_id).0
}

/// Derive the NFT-authority PDA: seeds = [b"nft_authority"].
fn nft_authority_pda(program_id: &Pubkey) -> Pubkey {
    get_pda_and_bump(&[Seed::from(b"nft_authority".as_ref())], program_id).0
}

/// Derive the associated token account for (wallet, mint) under Token Extensions.
fn associated_token_address(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[wallet.as_ref(), TOKEN_2022_ID.as_ref(), mint.as_ref()],
        &ASSOCIATED_TOKEN_ID,
    )
    .0
}

fn init_player_ix(program_id: &Pubkey, signer: &Pubkey) -> Instruction {
    Instruction {
        program_id: *program_id,
        accounts: extension_nft::accounts::InitPlayer {
            player: player_pda(program_id, signer),
            game_data: game_data_pda(program_id, LEVEL_SEED),
            signer: *signer,
            system_program: system_program::id(),
        }
        .to_account_metas(None),
        data: extension_nft::instruction::InitPlayer {
            _level_seed: LEVEL_SEED.to_string(),
        }
        .data(),
    }
}

fn mint_nft_ix(program_id: &Pubkey, signer: &Pubkey, mint: &Pubkey) -> Instruction {
    Instruction {
        program_id: *program_id,
        accounts: extension_nft::accounts::MintNft {
            signer: *signer,
            system_program: system_program::id(),
            token_program: TOKEN_2022_ID,
            token_account: associated_token_address(signer, mint),
            mint: *mint,
            rent: RENT_SYSVAR_ID,
            associated_token_program: ASSOCIATED_TOKEN_ID,
            nft_authority: nft_authority_pda(program_id),
        }
        .to_account_metas(None),
        data: extension_nft::instruction::MintNft {}.data(),
    }
}

fn chop_tree_ix(program_id: &Pubkey, signer: &Pubkey, mint: &Pubkey, counter: u16) -> Instruction {
    Instruction {
        program_id: *program_id,
        accounts: extension_nft::accounts::ChopTree {
            // session_token is optional; pass None -> the macro falls back to
            // the main-wallet authority check.
            session_token: None,
            player: player_pda(program_id, signer),
            game_data: game_data_pda(program_id, LEVEL_SEED),
            signer: *signer,
            system_program: system_program::id(),
            mint: *mint,
            nft_authority: nft_authority_pda(program_id),
            token_program: TOKEN_2022_ID,
        }
        .to_account_metas(None),
        data: extension_nft::instruction::ChopTree {
            _level_seed: LEVEL_SEED.to_string(),
            counter,
        }
        .data(),
    }
}

/// Decode the borsh `PlayerData` account (after the 8-byte discriminator).
struct Player {
    wood: u64,
    energy: u64,
}

fn fetch_player(svm: &LiteSVM, player: &Pubkey) -> Player {
    use anchor_lang::AnchorDeserialize;
    let account = svm.get_account(player).expect("player account exists");
    // Skip the 8-byte Anchor discriminator.
    let mut data = &account.data[8..];
    // PlayerData layout: authority(32) name(4+len) level(1) xp(8) wood(8)
    // energy(8) last_login(8) last_id(2) bump(1).
    let _authority = <[u8; 32]>::deserialize(&mut data).unwrap();
    let _name = String::deserialize(&mut data).unwrap();
    let _level = u8::deserialize(&mut data).unwrap();
    let _xp = u64::deserialize(&mut data).unwrap();
    let wood = u64::deserialize(&mut data).unwrap();
    let energy = u64::deserialize(&mut data).unwrap();
    Player { wood, energy }
}

#[test]
fn test_init_player_mint_and_chop() {
    let (mut svm, program_id) = setup();
    let payer = create_wallet(&mut svm, 100_000_000_000).unwrap();
    let signer = payer.pubkey();

    // 1. init_player
    send_transaction_from_instructions(
        &mut svm,
        vec![init_player_ix(&program_id, &signer)],
        &[&payer],
        &signer,
    )
    .expect("init_player should succeed");

    let player_addr = player_pda(&program_id, &signer);
    let player = fetch_player(&svm, &player_addr);
    assert_eq!(player.wood, 0, "fresh player starts with no wood");
    assert_eq!(
        player.energy, 100,
        "fresh player starts at max energy (100)"
    );

    // 2. mint_nft - the mint account is a fresh keypair (it's a Signer in the
    //    instruction because the program creates it via a system CPI).
    let mint = Keypair::new();
    send_transaction_from_instructions(
        &mut svm,
        vec![mint_nft_ix(&program_id, &signer, &mint.pubkey())],
        &[&payer, &mint],
        &signer,
    )
    .expect("mint_nft should succeed");

    // The mint account is now owned by the Token Extensions program and holds the
    // inline metadata extension, so it is comfortably larger than a bare mint.
    let mint_account = svm.get_account(&mint.pubkey()).expect("mint exists");
    assert_eq!(
        mint_account.owner, TOKEN_2022_ID,
        "mint owned by Token Extensions"
    );
    assert!(
        mint_account.data.len() > 82,
        "mint carries extension data (got {} bytes)",
        mint_account.data.len()
    );

    // The associated token account should exist and hold the single NFT.
    let ata = associated_token_address(&signer, &mint.pubkey());
    let ata_account = svm.get_account(&ata).expect("ATA created");
    assert_eq!(ata_account.owner, TOKEN_2022_ID, "ATA owned by Token Extensions");

    // 3. chop_tree - needs the existing mint so it can push the new wood total
    //    into the NFT metadata. Signed by the player's main wallet (no session).
    send_transaction_from_instructions(
        &mut svm,
        vec![chop_tree_ix(&program_id, &signer, &mint.pubkey(), 1)],
        &[&payer],
        &signer,
    )
    .expect("chop_tree should succeed");

    let player = fetch_player(&svm, &player_addr);
    assert_eq!(player.wood, 1, "player gained 1 wood from chopping");
    assert_eq!(player.energy, 99, "player spent 1 energy chopping");
}
