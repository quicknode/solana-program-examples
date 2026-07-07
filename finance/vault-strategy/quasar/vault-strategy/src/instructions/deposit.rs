use quasar_lang::prelude::*;
use quasar_lang::remaining::RemainingAccounts;
use quasar_lang::sysvars::Sysvar as _;
use quasar_spl::prelude::*;

use crate::errors::VaultError;
use crate::oracle::{asset_value_in_usdc, load_price, read_token_amount, PYTH_PRICE_PRECISION};
use crate::state::{
    load_asset_config, snapshot_strategy, ShareMintPda, Strategy, UsdcVaultPda, STRATEGY_SEED,
};

/// Discriminator of the router's `swap_usdc_for_asset` instruction.
const ROUTER_SWAP_USDC_FOR_ASSET: u8 = 2;
/// One `swap_usdc_for_asset` CPI: 10 accounts, 17 data bytes (disc + 2 u64).
const SWAP_ACCOUNTS: usize = 10;
const SWAP_DATA_LEN: usize = 17;
/// remaining_accounts arrive as, per asset index 0..asset_count:
///   [asset_config, vault, asset_mint, asset_rate, price_feed]
const ACCOUNTS_PER_ASSET: usize = 5;

#[derive(Accounts)]
pub struct DepositAccountConstraints {
    #[account(mut)]
    pub depositor: Signer,

    #[account(
        mut,
        address = Strategy::seeds(strategy.index.into()),
        has_one(usdc_mint) @ VaultError::InvalidUsdcMint,
    )]
    pub strategy: Account<Strategy>,

    #[account(mut, address = ShareMintPda::seeds(strategy.address()))]
    pub share_mint: InterfaceAccount<Mint>,

    pub usdc_mint: Account<Mint>,

    #[account(mut)]
    pub depositor_usdc_account: Account<Token>,

    // The depositor's share account. Must already exist and be owned by the
    // depositor (verified before shares are minted).
    #[account(mut)]
    pub depositor_share_account: Account<Token>,

    #[account(mut, address = UsdcVaultPda::seeds(strategy.address()))]
    pub vault_usdc: InterfaceAccount<Token>,

    /// Router config PDA (mock-swap-router).
    #[account(mut)]
    pub router_config: UncheckedAccount,
    /// Router USDC treasury.
    #[account(mut)]
    pub router_usdc_treasury: UncheckedAccount,
    /// Router mint-authority PDA.
    pub router_authority: UncheckedAccount,
    /// The swap router program, verified against the strategy's stored router.
    pub swap_router_program: UncheckedAccount,

    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

fn get_view<'a>(
    remaining: &RemainingAccounts<'a>,
    index: usize,
) -> Result<AccountView, ProgramError> {
    let account = remaining
        .get(index)?
        .ok_or(VaultError::IncompleteAssetAccounts)?;
    // SAFETY: forwarded/read-only; no mutable alias is taken across these views.
    Ok(unsafe { account.as_account_view_unchecked() }.clone())
}

#[inline(always)]
pub fn handle_deposit(
    accounts: &mut DepositAccountConstraints,
    remaining: RemainingAccounts<'_>,
    usdc_amount: u64,
    minimum_shares: u64,
) -> Result<(), ProgramError> {
    require!(usdc_amount > 0, VaultError::ZeroDeposit);
    require!(
        u16::from(accounts.strategy.total_weight_bps) == 10_000,
        VaultError::StrategyNotFullyAllocated
    );

    let asset_count = accounts.strategy.asset_count as usize;
    // Exactly five accounts per asset - no more, no less - so no asset can be
    // omitted from the NAV computation.
    require!(
        remaining.get(asset_count * ACCOUNTS_PER_ASSET)?.is_none(),
        VaultError::IncompleteAssetAccounts
    );

    let total_shares = u64::from(accounts.strategy.total_shares);
    let usdc_decimals = accounts.usdc_mint.decimals;
    let strategy_index = u64::from(accounts.strategy.index);
    let strategy_bump = accounts.strategy.bump;
    let strategy_key = *accounts.strategy.address();
    let max_slippage_bps = u16::from(accounts.strategy.max_slippage_bps);
    let router_program_addr = *accounts.swap_router_program.to_account_view().address();

    require_keys_eq!(
        router_program_addr,
        accounts.strategy.swap_router,
        VaultError::InvalidSwapRouter
    );

    let now = i64::from(Clock::get()?.unix_timestamp);

    // Net asset value over the complete asset set.
    let mut nav: u128 = accounts.vault_usdc.amount() as u128;
    for index in 0..asset_count {
        let config_view = get_view(&remaining, index * ACCOUNTS_PER_ASSET)?;
        let vault_view = get_view(&remaining, index * ACCOUNTS_PER_ASSET + 1)?;
        let feed_view = get_view(&remaining, index * ACCOUNTS_PER_ASSET + 4)?;

        let config = load_asset_config(&config_view)?;
        require_keys_eq!(config.strategy, strategy_key, VaultError::InvalidAssetAccount);
        require!(config.index as usize == index, VaultError::InvalidAssetAccount);
        require_keys_eq!(*vault_view.address(), config.vault, VaultError::InvalidAssetAccount);

        let price = load_price(&feed_view, &config.price_feed, now)?;
        let amount = read_token_amount(&vault_view)?;
        nav = nav
            .checked_add(asset_value_in_usdc(amount, price)?)
            .ok_or(VaultError::MathOverflow)?;
    }

    // shares = usdc_amount * total_shares / nav (floor); first deposit is 1:1.
    let shares_to_mint: u64 = if total_shares == 0 {
        usdc_amount
    } else {
        (usdc_amount as u128)
            .checked_mul(total_shares as u128)
            .ok_or(VaultError::MathOverflow)?
            .checked_div(nav)
            .ok_or(VaultError::MathOverflow)?
            .try_into()
            .map_err(|_| VaultError::MathOverflow)?
    };
    require!(shares_to_mint >= minimum_shares, VaultError::SlippageTooHigh);

    let mut strategy = snapshot_strategy(&accounts.strategy);
    strategy.total_shares = total_shares
        .checked_add(shares_to_mint)
        .ok_or(VaultError::MathOverflow)?;
    accounts.strategy.set_inner(strategy);

    // Pull the depositor's USDC into the strategy's USDC vault.
    accounts
        .token_program
        .transfer_checked(
            &accounts.depositor_usdc_account,
            &accounts.usdc_mint,
            &accounts.vault_usdc,
            &accounts.depositor,
            usdc_amount,
            usdc_decimals,
        )
        .invoke()?;

    let index_bytes = strategy_index.to_le_bytes();
    let bump = [strategy_bump];
    let seeds = [
        Seed::from(STRATEGY_SEED),
        Seed::from(index_bytes.as_ref()),
        Seed::from(bump.as_ref()),
    ];

    // Deploy the deposit across the basket at its target weights, each leg
    // swapped through the router under an oracle-anchored slippage floor. The
    // strategy PDA signs, since the USDC leaves a vault only it controls.
    for index in 0..asset_count {
        let config_view = get_view(&remaining, index * ACCOUNTS_PER_ASSET)?;
        let vault_view = get_view(&remaining, index * ACCOUNTS_PER_ASSET + 1)?;
        let mint_view = get_view(&remaining, index * ACCOUNTS_PER_ASSET + 2)?;
        let rate_view = get_view(&remaining, index * ACCOUNTS_PER_ASSET + 3)?;
        let feed_view = get_view(&remaining, index * ACCOUNTS_PER_ASSET + 4)?;

        let config = load_asset_config(&config_view)?;
        require_keys_eq!(*mint_view.address(), config.mint, VaultError::InvalidAssetAccount);

        if config.weight_bps == 0 {
            continue;
        }
        let deploy_usdc: u64 = (usdc_amount as u128)
            .checked_mul(config.weight_bps as u128)
            .ok_or(VaultError::MathOverflow)?
            .checked_div(10_000)
            .ok_or(VaultError::MathOverflow)?
            .try_into()
            .map_err(|_| VaultError::MathOverflow)?;
        if deploy_usdc == 0 {
            continue;
        }

        // Oracle-anchored floor: expected_out = deploy_usdc * 10^8 / price,
        // allowed to fall short by at most max_slippage_bps.
        let price = load_price(&feed_view, &config.price_feed, now)?;
        let expected_out = (deploy_usdc as u128)
            .checked_mul(PYTH_PRICE_PRECISION)
            .ok_or(VaultError::MathOverflow)?
            .checked_div(price)
            .ok_or(VaultError::MathOverflow)?;
        let minimum_asset_out: u64 = expected_out
            .checked_mul((10_000 - max_slippage_bps) as u128)
            .ok_or(VaultError::MathOverflow)?
            .checked_div(10_000)
            .ok_or(VaultError::MathOverflow)?
            .try_into()
            .map_err(|_| VaultError::MathOverflow)?;

        let mut data = [0u8; SWAP_DATA_LEN];
        data[0] = ROUTER_SWAP_USDC_FOR_ASSET;
        data[1..9].copy_from_slice(&deploy_usdc.to_le_bytes());
        data[9..17].copy_from_slice(&minimum_asset_out.to_le_bytes());

        // Router `swap_usdc_for_asset` account order:
        //   caller, router_config, asset_rate, usdc_mint, asset_mint,
        //   caller_usdc_account, caller_asset_account, router_usdc_treasury,
        //   router_authority, token_program.
        let mut cpi = CpiDynamic::<SWAP_ACCOUNTS, SWAP_DATA_LEN>::new(&router_program_addr);
        cpi.push_account(accounts.strategy.to_account_view(), true, false)?;
        cpi.push_account(accounts.router_config.to_account_view(), false, false)?;
        cpi.push_account(&rate_view, false, false)?;
        cpi.push_account(accounts.usdc_mint.to_account_view(), false, false)?;
        cpi.push_account(&mint_view, false, true)?;
        cpi.push_account(accounts.vault_usdc.to_account_view(), false, true)?;
        cpi.push_account(&vault_view, false, true)?;
        cpi.push_account(accounts.router_usdc_treasury.to_account_view(), false, true)?;
        cpi.push_account(accounts.router_authority.to_account_view(), false, false)?;
        cpi.push_account(accounts.token_program.to_account_view(), false, false)?;
        cpi.set_data(&data)?;
        cpi.invoke_signed(&seeds)?;
    }

    // Mint the shares last, with the strategy PDA signing as the share-mint
    // authority; the depositor's share account must belong to the depositor.
    require_keys_eq!(
        accounts.depositor_share_account.owner,
        *accounts.depositor.address(),
        VaultError::InvalidRecipient
    );
    accounts
        .token_program
        .mint_to(
            &accounts.share_mint,
            &accounts.depositor_share_account,
            &accounts.strategy,
            shares_to_mint,
        )
        .invoke_signed(&seeds)?;

    Ok(())
}
