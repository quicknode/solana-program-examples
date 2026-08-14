mod common;

use anchor_lang::{
    solana_program::instruction::Instruction, system_program, InstructionData, ToAccountMetas,
};
use common::{default_config, dollars, Env};
use solana_signer::Signer;

/// A reserve from one lending market cannot be used with an obligation from
/// another: lending markets are isolation boundaries.
#[test]
fn cross_market_reserve_is_rejected() {
    let mut env = Env::new();
    let collateral = env.add_reserve(6, dollars(1), default_config());

    // A second market with its own reserve.
    let other_owner = env.create_user();
    let other_market = env.init_market_for(&other_owner);
    let foreign_reserve =
        env.add_reserve_to(&other_owner, other_market, 6, dollars(1), default_config());

    // A borrower set up in the FIRST market.
    let borrower = env.create_user();
    env.fund(&borrower, collateral.mint, 1_000_000_000);
    env.supply(&borrower, &collateral, 1_000_000_000);
    let obligation = env.initialize_obligation(&borrower);

    // Posting collateral via the second market's reserve must fail before any
    // token movement.
    env.fund(&borrower, foreign_reserve.share_mint, 0); // create the share ATA
    let result = env.try_post_collateral(&borrower, obligation, &foreign_reserve, 1);
    assert!(
        result.unwrap_err().contains("MarketMismatch"),
        "a reserve from another lending market must be rejected"
    );
}

/// A market's price feed can only be written by that market's owner: an
/// outsider cannot publish (or squat) prices for a market they don't own.
#[test]
fn non_owner_cannot_write_market_price_feed() {
    let mut env = Env::new();
    let usdc = env.add_reserve(6, dollars(1), default_config());

    let attacker = env.create_user();
    // The market's feed for this mint (seeds ["price_feed", market, mint]).
    let market_feed = env.price_feed_address(env.market, usdc.mint);

    // The attacker passes the real market but signs as themself; `address = lending_market.owner`
    // on the market rejects them before any write.
    let instruction = Instruction {
        program_id: lending::id(),
        accounts: lending::accounts::SetPrice {
            lending_market: env.market,
            owner: attacker.pubkey(),
            price_feed: market_feed,
            mint: usdc.mint,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
        data: lending::instruction::SetPrice {
            price_mantissa: common::dollars(1_000_000), // an absurd price
            exponent: common::PRICE_EXPONENT,
        }
        .data(),
    };
    let result = solana_kite::send_transaction_from_instructions(
        &mut env.svm,
        vec![instruction],
        &[&attacker],
        &attacker.pubkey(),
    );
    assert!(
        result.is_err(),
        "only the market owner may write its price feed"
    );
}
