//! Stop-loss vault scenarios.
//!
//! Each test is a story with named actors (Alice, Bob, Carol, Dave), real
//! numbers (USDC 6 decimals, NVDAx 9 decimals, oracle 8 decimals), and a clear
//! beat-by-beat progression a non-engineer can follow.

// `anchor build` generates the IDL by compiling the crate with the `idl-build`
// feature, which also compiles this integration test. The test `include_bytes!`s
// the three program `.so` files, which don't exist yet during IDL generation —
// so exclude the whole harness from `idl-build`. It still compiles and runs
// under `cargo test` (where `idl-build` is off and the `.so` files exist).
#![cfg(not(feature = "idl-build"))]

mod common;

use common::*;
use solana_signer::Signer;

/// Alice opens a stop-loss vault. NVDAx is at $200, threshold is $100, crank
/// cadence is every 10 minutes. Alice deposits 10 NVDAx into the vault.
#[test]
fn test_alice_initialises_vault_with_100_usd_threshold() {
    let mut world = new_world();

    // NVDAx @ $200 expressed in the oracle's 8-decimal scale.
    let starting_price = dollars_to_oracle_price(200);
    initialize_feed(&mut world, starting_price);

    let alice = new_funded_keypair(&mut world.svm, sol(10));
    let alice_volatile_ata = fund_with_volatile(&mut world, &alice, nvdax(10));
    let _alice_stable_ata = create_stable_ata(&mut world, &alice);

    // Threshold $100 in oracle scale. Crank every 10 minutes.
    let threshold = dollars_to_oracle_price(100);
    let crank_interval_seconds = 600;
    let vault = initialize_vault(&mut world, &alice, threshold, crank_interval_seconds);

    deposit(&mut world, &alice, nvdax(10));

    // Vault now holds Alice's 10 NVDAx.
    let vault_volatile_ata =
        anchor_spl::associated_token::get_associated_token_address(&vault, &world.volatile_mint);
    assert_eq!(token_balance(&world.svm, &vault_volatile_ata), nvdax(10));
    // Alice's personal volatile ATA is empty.
    assert_eq!(token_balance(&world.svm, &alice_volatile_ata), 0);

    let state = vault_state(&world.svm, &vault);
    assert_eq!(state.owner, alice.pubkey());
    assert_eq!(state.threshold_price, threshold);
    assert_eq!(state.crank_interval_seconds, crank_interval_seconds);
    assert!(!state.triggered);
}

/// Alice's vault is armed at $100. Bob is the cranker. Hour 1 NVDAx is at $180:
/// Bob's crank reverts. Hour 2 at $150: reverts. Hour 3 at $80: conversion
/// fires. Vault volatile balance \u2192 0, vault stable balance \u2192 10 * $80 = $800
/// USDC (six decimals).
#[test]
fn test_price_drops_below_threshold_on_third_check_converts_to_stables() {
    let mut world = new_world();

    initialize_feed(&mut world, dollars_to_oracle_price(200));
    let alice = new_funded_keypair(&mut world.svm, sol(10));
    let _ = fund_with_volatile(&mut world, &alice, nvdax(10));
    let _ = create_stable_ata(&mut world, &alice);
    let vault = initialize_vault(&mut world, &alice, dollars_to_oracle_price(100), 600);
    deposit(&mut world, &alice, nvdax(10));

    let bob = new_funded_keypair(&mut world.svm, sol(1));
    let vault_volatile =
        anchor_spl::associated_token::get_associated_token_address(&vault, &world.volatile_mint);
    let vault_stable =
        anchor_spl::associated_token::get_associated_token_address(&vault, &world.stable_mint);

    // Hour 1: price $180. Bob cranks, reverts because price > threshold.
    set_feed_price(&mut world, dollars_to_oracle_price(180));
    let result = try_convert_if_triggered(&mut world, &bob, &alice.pubkey());
    assert!(result.is_err(), "expected revert at $180 > $100 threshold");
    assert_eq!(token_balance(&world.svm, &vault_volatile), nvdax(10));
    assert_eq!(token_balance(&world.svm, &vault_stable), 0);

    // Hour 2: price $150. Same story.
    set_feed_price(&mut world, dollars_to_oracle_price(150));
    let result = try_convert_if_triggered(&mut world, &bob, &alice.pubkey());
    assert!(result.is_err(), "expected revert at $150 > $100 threshold");
    assert_eq!(token_balance(&world.svm, &vault_volatile), nvdax(10));

    // Hour 3: price $80. NOW the crank fires.
    set_feed_price(&mut world, dollars_to_oracle_price(80));
    try_convert_if_triggered(&mut world, &bob, &alice.pubkey())
        .expect("crank should succeed when price drops below threshold");

    // Vault has been drained of volatile and now holds stables: 10 NVDAx * $80 = $800.
    assert_eq!(token_balance(&world.svm, &vault_volatile), 0);
    assert_eq!(token_balance(&world.svm, &vault_stable), usdc(800));

    let state = vault_state(&world.svm, &vault);
    assert!(state.triggered);

    // Alice can now withdraw her stables.
    try_withdraw_stables(&mut world, &alice, &alice.pubkey(), usdc(800))
        .expect("Alice should be able to withdraw stables after the trigger");
    let alice_stable =
        anchor_spl::associated_token::get_associated_token_address(&alice.pubkey(), &world.stable_mint);
    assert_eq!(token_balance(&world.svm, &alice_stable), usdc(800));
    assert_eq!(token_balance(&world.svm, &vault_stable), 0);
}

/// Carol does not own any vault. She tries to withdraw stables from a vault
/// that doesn't exist for her, then from Alice's vault by passing Alice's
/// owner key. Both should fail \u2014 Anchor's signer + has_one + PDA seeds keep
/// the funds safe.
#[test]
fn test_carol_cannot_withdraw_someone_elses_vault() {
    let mut world = new_world();

    initialize_feed(&mut world, dollars_to_oracle_price(200));
    let alice = new_funded_keypair(&mut world.svm, sol(10));
    let _ = fund_with_volatile(&mut world, &alice, nvdax(10));
    let _ = create_stable_ata(&mut world, &alice);
    let _vault = initialize_vault(&mut world, &alice, dollars_to_oracle_price(100), 600);
    deposit(&mut world, &alice, nvdax(10));

    // Trigger the vault so it actually has stables to steal.
    set_feed_price(&mut world, dollars_to_oracle_price(80));
    let bob = new_funded_keypair(&mut world.svm, sol(1));
    try_convert_if_triggered(&mut world, &bob, &alice.pubkey()).unwrap();

    // Carol enters.
    let carol = new_funded_keypair(&mut world.svm, sol(1));
    let _ = create_stable_ata(&mut world, &carol);

    // Path 1: Carol asks the vault PDA derived from HER OWN owner key. There
    // is no vault at that PDA \u2014 deserialise fails.
    let result = try_withdraw_stables(&mut world, &carol, &carol.pubkey(), usdc(100));
    assert!(
        result.is_err(),
        "Carol with her own owner key shouldn't find a vault"
    );

    // Path 2: Carol points at Alice's vault PDA but signs as herself. The
    // `has_one = owner` check on the vault fails because Carol's signer is
    // not the vault's owner.
    let result = try_withdraw_stables(&mut world, &carol, &alice.pubkey(), usdc(100));
    assert!(
        result.is_err(),
        "Carol shouldn't be able to drain Alice's vault"
    );
}

/// Alice's vault was opened at $100 threshold when NVDAx was $200. NVDAx rallies
/// to $250. Alice trails the threshold UP to $200 \u2014 she now wants
/// protection at a higher floor. Bob's crank at $220 still reverts. NVDAx
/// finally falls to $180, Bob cranks, conversion fires at the new threshold.
#[test]
fn test_alice_trails_threshold_up_as_price_rises() {
    let mut world = new_world();

    initialize_feed(&mut world, dollars_to_oracle_price(200));
    let alice = new_funded_keypair(&mut world.svm, sol(10));
    let _ = fund_with_volatile(&mut world, &alice, nvdax(10));
    let _ = create_stable_ata(&mut world, &alice);
    let vault = initialize_vault(&mut world, &alice, dollars_to_oracle_price(100), 600);
    deposit(&mut world, &alice, nvdax(10));

    let vault_stable =
        anchor_spl::associated_token::get_associated_token_address(&vault, &world.stable_mint);

    // Price rallies to $250. Alice trails the threshold up to $200.
    set_feed_price(&mut world, dollars_to_oracle_price(250));
    try_update_threshold(&mut world, &alice, Some(dollars_to_oracle_price(200)), None)
        .expect("Alice should be able to update her own threshold");
    let state = vault_state(&world.svm, &vault);
    assert_eq!(state.threshold_price, dollars_to_oracle_price(200));

    let bob = new_funded_keypair(&mut world.svm, sol(1));

    // Bob cranks at $220 \u2014 still above $200 threshold.
    set_feed_price(&mut world, dollars_to_oracle_price(220));
    let result = try_convert_if_triggered(&mut world, &bob, &alice.pubkey());
    assert!(result.is_err(), "crank at $220 vs $200 threshold must revert");

    // Price falls to $180. Now below the trailed threshold.
    set_feed_price(&mut world, dollars_to_oracle_price(180));
    try_convert_if_triggered(&mut world, &bob, &alice.pubkey())
        .expect("crank at $180 vs $200 threshold should fire");

    // 10 NVDAx * $180 = $1800.
    assert_eq!(token_balance(&world.svm, &vault_stable), usdc(1800));
}

/// Bob runs the cranker when NVDAx is well above the threshold. The
/// instruction reverts with `PriceAboveThreshold` and leaves vault state
/// untouched \u2014 importantly the vault is NOT marked triggered.
#[test]
fn test_price_above_threshold_crank_reverts_cheaply() {
    let mut world = new_world();

    initialize_feed(&mut world, dollars_to_oracle_price(300));
    let alice = new_funded_keypair(&mut world.svm, sol(10));
    let _ = fund_with_volatile(&mut world, &alice, nvdax(10));
    let _ = create_stable_ata(&mut world, &alice);
    let vault = initialize_vault(&mut world, &alice, dollars_to_oracle_price(100), 600);
    deposit(&mut world, &alice, nvdax(10));

    let vault_volatile =
        anchor_spl::associated_token::get_associated_token_address(&vault, &world.volatile_mint);
    let vault_stable =
        anchor_spl::associated_token::get_associated_token_address(&vault, &world.stable_mint);

    let bob = new_funded_keypair(&mut world.svm, sol(1));
    set_feed_price(&mut world, dollars_to_oracle_price(300));

    let result = try_convert_if_triggered(&mut world, &bob, &alice.pubkey());
    assert!(result.is_err(), "crank at $300 vs $100 threshold must revert");

    // State after the failed crank: vault untouched, not triggered.
    let state = vault_state(&world.svm, &vault);
    assert!(!state.triggered, "vault must NOT be marked triggered on a no-op crank");
    assert_eq!(token_balance(&world.svm, &vault_volatile), nvdax(10));
    assert_eq!(token_balance(&world.svm, &vault_stable), 0);
}

/// Documents the flash-crash limitation.
///
/// Alice's vault is armed at $100. Hour 1 the price is $200, Bob cranks and
/// reverts. BETWEEN the hour-1 and hour-2 cranks the price flash-crashes to
/// $50 and recovers to $180 before Bob's next chance to look. Hour 2 Bob
/// cranks at $180 \u2014 still above threshold \u2014 reverts. The vault was NOT
/// converted, even though the price went below the threshold mid-window.
///
/// This is a known limitation of any discrete-time onchain stop-loss: the
/// program only sees the price at crank time. The fix in real systems is
/// either a tighter crank cadence (more expensive) or a continuous-watch
/// offchain liquidator (worse trust assumptions). We document the gap
/// instead of pretending it doesn't exist.
#[test]
fn test_flash_crash_between_cranks_misses_trigger() {
    let mut world = new_world();

    initialize_feed(&mut world, dollars_to_oracle_price(200));
    let alice = new_funded_keypair(&mut world.svm, sol(10));
    let _ = fund_with_volatile(&mut world, &alice, nvdax(10));
    let _ = create_stable_ata(&mut world, &alice);
    let vault = initialize_vault(&mut world, &alice, dollars_to_oracle_price(100), 600);
    deposit(&mut world, &alice, nvdax(10));

    let vault_volatile =
        anchor_spl::associated_token::get_associated_token_address(&vault, &world.volatile_mint);
    let vault_stable =
        anchor_spl::associated_token::get_associated_token_address(&vault, &world.stable_mint);

    let bob = new_funded_keypair(&mut world.svm, sol(1));

    // Hour 1: $200, well above threshold. Bob cranks, reverts.
    set_feed_price(&mut world, dollars_to_oracle_price(200));
    let result = try_convert_if_triggered(&mut world, &bob, &alice.pubkey());
    assert!(result.is_err());

    // Between cranks: $50 (would have triggered), back to $180. The vault
    // never sees the $50 print because no crank fires while it's there.
    set_feed_price(&mut world, dollars_to_oracle_price(50));
    set_feed_price(&mut world, dollars_to_oracle_price(180));

    // Hour 2: Bob cranks at $180. Still above threshold \u2014 reverts.
    let result = try_convert_if_triggered(&mut world, &bob, &alice.pubkey());
    assert!(result.is_err());

    // Despite the price having been below threshold during the window, the
    // vault is NOT converted. This is the limitation.
    let state = vault_state(&world.svm, &vault);
    assert!(!state.triggered, "flash-crash between cranks does NOT trigger - known limitation");
    assert_eq!(token_balance(&world.svm, &vault_volatile), nvdax(10));
    assert_eq!(token_balance(&world.svm, &vault_stable), 0);
}

/// NVDAx never falls to Alice's threshold, so the vault never triggers. Alice
/// changes her mind and pulls her 10 NVDAx back out with `withdraw_volatile` —
/// the escape hatch that stops a never-triggered vault from locking the
/// deposit forever. Carol cannot use it against Alice's vault.
#[test]
fn test_owner_withdraws_volatile_before_trigger() {
    let mut world = new_world();

    initialize_feed(&mut world, dollars_to_oracle_price(200));
    let alice = new_funded_keypair(&mut world.svm, sol(10));
    let alice_volatile = fund_with_volatile(&mut world, &alice, nvdax(10));
    let _ = create_stable_ata(&mut world, &alice);
    let vault = initialize_vault(&mut world, &alice, dollars_to_oracle_price(100), 600);
    deposit(&mut world, &alice, nvdax(10));

    let vault_volatile =
        anchor_spl::associated_token::get_associated_token_address(&vault, &world.volatile_mint);
    assert_eq!(token_balance(&world.svm, &vault_volatile), nvdax(10));

    // Carol can't drain Alice's volatile balance.
    let carol = new_funded_keypair(&mut world.svm, sol(1));
    let _ = create_stable_ata(&mut world, &carol);
    let result = try_withdraw_volatile(&mut world, &carol, &alice.pubkey(), nvdax(10));
    assert!(result.is_err(), "Carol must not withdraw Alice's volatile balance");

    // Alice pulls her full deposit back out.
    try_withdraw_volatile(&mut world, &alice, &alice.pubkey(), nvdax(10))
        .expect("owner should be able to withdraw volatile before a trigger");
    assert_eq!(token_balance(&world.svm, &vault_volatile), 0);
    assert_eq!(token_balance(&world.svm, &alice_volatile), nvdax(10));

    let state = vault_state(&world.svm, &vault);
    assert!(!state.triggered);
}

/// The price drops below the threshold, but no crank runs for a long time and
/// the feed goes stale. When Bob finally cranks, `convert_if_triggered` rejects
/// the stale price instead of swapping on it: a feed that has stopped updating
/// must not move the vault's funds.
#[test]
fn test_stale_price_crank_reverts() {
    let mut world = new_world();

    initialize_feed(&mut world, dollars_to_oracle_price(200));
    let alice = new_funded_keypair(&mut world.svm, sol(10));
    let _ = fund_with_volatile(&mut world, &alice, nvdax(10));
    let _ = create_stable_ata(&mut world, &alice);
    let vault = initialize_vault(&mut world, &alice, dollars_to_oracle_price(100), 600);
    deposit(&mut world, &alice, nvdax(10));

    let vault_volatile =
        anchor_spl::associated_token::get_associated_token_address(&vault, &world.volatile_mint);
    let vault_stable =
        anchor_spl::associated_token::get_associated_token_address(&vault, &world.stable_mint);

    // Price drops to $80 — below the $100 threshold, so a fresh crank would
    // fire. But the feed then stops updating for longer than the staleness
    // bound.
    set_feed_price(&mut world, dollars_to_oracle_price(80));
    warp_past_price_staleness(&mut world);

    let bob = new_funded_keypair(&mut world.svm, sol(1));
    let result = try_convert_if_triggered(&mut world, &bob, &alice.pubkey());
    assert!(result.is_err(), "a stale price must not trigger a conversion");

    let state = vault_state(&world.svm, &vault);
    assert!(!state.triggered, "stale price leaves the vault un-triggered");
    assert_eq!(token_balance(&world.svm, &vault_volatile), nvdax(10));
    assert_eq!(token_balance(&world.svm, &vault_stable), 0);
}
