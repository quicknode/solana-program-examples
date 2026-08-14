mod common;

use common::{default_config, Env};
use solana_kite::mint_tokens_to_token_account;

#[test]
fn first_deposit_mints_shares_one_to_one() {
    let mut env = Env::new();
    let usdc = env.add_reserve(6, common::dollars(1), default_config());

    let supplier = env.create_user();
    let deposit = 1_000_000_000; // 1000 USDC
    env.fund(&supplier, usdc.mint, deposit);
    let share_account = env.supply(&supplier, &usdc, deposit);

    assert_eq!(env.token_balance(share_account), deposit);
    let reserve = env.reserve(&usdc);
    assert_eq!(reserve.available_liquidity, deposit);
    assert_eq!(reserve.share_mint_supply, deposit);
}

#[test]
fn raw_token_donation_does_not_inflate_exchange_rate() {
    let mut env = Env::new();
    let usdc = env.add_reserve(6, common::dollars(1), default_config());

    let first = env.create_user();
    let amount = 1_000_000_000;
    env.fund(&first, usdc.mint, amount);
    env.supply(&first, &usdc, amount);

    // Attacker donates raw tokens straight into the reserve vault. available_liquidity
    // is the source of truth, so this must NOT change the share exchange rate.
    let owner = env.owner.insecure_clone();
    mint_tokens_to_token_account(
        &mut env.svm,
        &usdc.mint,
        &usdc.liquidity_vault,
        amount,
        &owner,
    )
    .unwrap();

    let second = env.create_user();
    env.fund(&second, usdc.mint, amount);
    let second_shares = env.supply(&second, &usdc, amount);

    // Despite the donation, the second supplier still gets 1:1 shares.
    assert_eq!(env.token_balance(second_shares), amount);
}

#[test]
fn redeem_returns_underlying_liquidity() {
    let mut env = Env::new();
    let usdc = env.add_reserve(6, common::dollars(1), default_config());

    let supplier = env.create_user();
    let amount = 1_000_000_000;
    let liquidity_account = env.fund(&supplier, usdc.mint, amount);
    let share_account = env.supply(&supplier, &usdc, amount);
    assert_eq!(env.token_balance(liquidity_account), 0);

    env.try_redeem(&supplier, &usdc, amount).unwrap();
    assert_eq!(env.token_balance(liquidity_account), amount);
    assert_eq!(env.token_balance(share_account), 0);
    assert_eq!(env.reserve(&usdc).share_mint_supply, 0);
}
