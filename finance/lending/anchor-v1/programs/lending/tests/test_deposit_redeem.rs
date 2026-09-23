mod common;

use common::{default_config, Env};
use lending::constants::MINIMUM_SHARES;
use solana_kite::mint_tokens_to_token_account;

/// The first deposit mints shares one-for-one, less the `MINIMUM_SHARES`
/// withheld. The withheld shares are never minted, so the tracked supply is
/// what the depositor holds, while the pool holds the whole deposit.
#[test]
fn first_deposit_mints_shares_one_to_one_less_the_minimum() {
    let mut env = Env::new();
    let usdc = env.add_empty_reserve(6, common::dollars(1), default_config());

    let supplier = env.create_user();
    let deposit = 1_000_000_000; // 1000 USDC
    env.fund(&supplier, usdc.mint, deposit);
    let share_account = env.supply(&supplier, &usdc, deposit);

    assert_eq!(env.token_balance(share_account), deposit - MINIMUM_SHARES);
    let reserve = env.reserve(&usdc);
    assert_eq!(reserve.available_liquidity, deposit);
    assert_eq!(reserve.share_mint_supply, deposit - MINIMUM_SHARES);
}

/// A first deposit no larger than the minimum would mint nothing, so it is
/// refused.
#[test]
fn first_deposit_must_exceed_the_minimum() {
    let mut env = Env::new();
    let usdc = env.add_empty_reserve(6, common::dollars(1), default_config());

    let supplier = env.create_user();
    env.fund(&supplier, usdc.mint, MINIMUM_SHARES);
    let result = env.try_supply(&supplier, &usdc, MINIMUM_SHARES);
    assert!(
        result.unwrap_err().contains("DepositTooSmall"),
        "a first deposit of only the minimum mints nothing and must be rejected"
    );
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
}

/// Even as the reserve's only supplier, the first depositor reclaims their
/// deposit less the withheld minimum: those shares belong to nobody, and their
/// liquidity stays in the pool. The next supplier is priced against that slice
/// rather than bootstrapped, at the same one share per unit.
#[test]
fn sole_supplier_leaves_the_minimum_behind() {
    let mut env = Env::new();
    let usdc = env.add_empty_reserve(6, common::dollars(1), default_config());

    let supplier = env.create_user();
    let amount = 1_000_000_000;
    let liquidity_account = env.fund(&supplier, usdc.mint, amount);
    let share_account = env.supply(&supplier, &usdc, amount);
    let shares = env.token_balance(share_account);
    env.try_redeem(&supplier, &usdc, shares).unwrap();

    assert_eq!(
        env.token_balance(liquidity_account),
        amount - MINIMUM_SHARES
    );
    let reserve = env.reserve(&usdc);
    assert_eq!(reserve.share_mint_supply, 0);
    assert_eq!(reserve.available_liquidity, MINIMUM_SHARES);

    let next = env.create_user();
    env.fund(&next, usdc.mint, 5_000);
    let next_shares = env.supply(&next, &usdc, 5_000);
    assert_eq!(env.token_balance(next_shares), 5_000);
}

/// First-depositor share inflation without a donation. Shares are priced
/// against tracked `total_liquidity`, not the vault balance, so tokens sent
/// straight to the vault move nothing. But `total_liquidity` also counts
/// interest owed on borrows, and a supplier can borrow from their own reserve.
///
/// The attacker opens the reserve holding a single share, borrows one base
/// unit of it against collateral in another reserve, and lets one second pass.
/// Debt is rounded up, so the one unit now reads as two and the lone share is
/// worth two units without having minted anything. From there, deposits and
/// redemptions that round down in the pool's favour ratchet the price up: each
/// deposit is the largest that still mints one share, and redeeming that share
/// leaves the rounding behind for the only other share, the attacker's own. A
/// deposit that would mint zero shares is refused (`DepositTooSmall`), so the
/// victim is not robbed outright; instead a deposit just under two shares'
/// worth mints one, and the attacker's share redeems half the pool.
///
/// The rate curve is the default one. Nothing about the attack needs the
/// market owner's cooperation or bad debt: the attacker repays what they owe.
///
/// `MINIMUM_SHARES` counts as shares nobody holds in every share conversion,
/// so the attacker's one share is 1 of 1_001 and whatever the rounding leaves
/// behind is spread mostly across shares they cannot redeem.
#[test]
fn inflating_shares_through_own_borrow_does_not_pay() {
    let mut env = Env::new();
    let usdc = env.add_empty_reserve(6, common::dollars(1), default_config());
    let collateral = env.add_reserve(6, common::dollars(1), default_config());

    let attacker = env.create_user();
    let budget: u64 = 4_000_000_000;
    let attacker_liquidity = env.fund(&attacker, usdc.mint, budget);

    // Open the reserve and keep exactly one share.
    let attacker_share = env.supply(&attacker, &usdc, MINIMUM_SHARES + 1);
    let surplus = env.token_balance(attacker_share) - 1;
    if surplus > 0 {
        env.try_redeem(&attacker, &usdc, surplus).unwrap();
    }
    assert_eq!(env.token_balance(attacker_share), 1);

    // Borrow one base unit against collateral in another reserve, and let a
    // second of interest round that debt up to two.
    env.fund(&attacker, collateral.mint, 1_000_000_000);
    let collateral_share = env.supply(&attacker, &collateral, 1_000_000_000);
    let obligation = env.initialize_obligation(&attacker);
    let collateral_shares = env.token_balance(collateral_share);
    env.post_collateral(&attacker, obligation, &collateral, collateral_shares);
    env.try_borrow(&attacker, obligation, &[&collateral], &[], &usdc, 1)
        .unwrap();
    env.warp_seconds(1);
    env.refresh_reserve_only(&attacker, &usdc);
    assert_eq!(env.reserve(&usdc).current_borrowed_amount().unwrap(), 2);

    // Ratchet until one share is worth more than half the victim's deposit.
    let victim_deposit: u64 = 1_000_000_000;
    for _ in 0..64 {
        let reserve = env.reserve(&usdc);
        let total = reserve.total_liquidity().unwrap();
        let shares = reserve.share_mint_supply as u128 + MINIMUM_SHARES as u128;
        if total * 2 > victim_deposit as u128 * shares {
            break;
        }
        // The largest deposit that mints exactly one share.
        let deposit = (2 * total).div_ceil(shares) - 1;
        env.svm.expire_blockhash();
        env.supply(&attacker, &usdc, deposit as u64);
        env.try_redeem(&attacker, &usdc, 1).unwrap();
    }

    let victim = env.create_user();
    let victim_liquidity = env.fund(&victim, usdc.mint, victim_deposit);
    let victim_share = env.supply(&victim, &usdc, victim_deposit);

    // The attacker exits: redeems their share and repays the debt.
    env.svm.expire_blockhash();
    let attacker_shares = env.token_balance(attacker_share);
    env.try_redeem(&attacker, &usdc, attacker_shares).unwrap();
    env.repay(&attacker, obligation, &usdc, 10);
    assert_eq!(env.reserve(&usdc).borrowed_principal, 0);
    let attacker_end = env.token_balance(attacker_liquidity);
    assert!(
        attacker_end <= budget,
        "attacker started with {budget} and ended with {attacker_end}"
    );

    // The victim exits with everything and gets back all but a sliver.
    let victim_shares = env.token_balance(victim_share);
    env.try_redeem(&victim, &usdc, victim_shares).unwrap();
    let victim_back = env.token_balance(victim_liquidity);
    assert!(
        victim_back * 1_000 >= victim_deposit * 999,
        "victim deposited {victim_deposit} and got back {victim_back}"
    );
}
