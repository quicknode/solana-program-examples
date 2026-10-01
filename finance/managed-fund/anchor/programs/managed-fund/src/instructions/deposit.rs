use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        mint_to, transfer_checked, Mint, MintTo, TokenAccount, TokenInterface, TransferChecked,
    },
};
use mock_swap_router::cpi::accounts::SwapUsdcForAssetAccountConstraints as RouterSwapAccounts;

use crate::error::FundError;
use crate::oracle::{asset_value_in_usdc, load_price, read_token_amount, usdc_to_asset_amount};
use crate::state::{AssetConfig, Fund};

#[derive(Accounts)]
pub struct DepositAccountConstraints {
    #[account(mut)]
    pub depositor: Signer,

    #[account(
        mut,
        seeds = [b"fund", fund.index.to_le_bytes()],
        bump = fund.bump,
    )]
    pub fund: Box<BorshAccount<Fund>>,

    #[account(
        mut,
        seeds = [b"share_mint", fund.address().as_ref()],
        bump
    )]
    pub share_mint: Box<InterfaceAccount<Mint>>,

    #[account(address = fund.usdc_mint @ FundError::InvalidUsdcMint)]
    pub usdc_mint: Box<InterfaceAccount<Mint>>,

    #[account(
        mut,
        associated_token::mint = usdc_mint,
        associated_token::authority = depositor,
        associated_token::token_program = token_program
    )]
    pub depositor_usdc_account: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        init_if_needed,
        payer = depositor,
        associated_token::mint = share_mint,
        associated_token::authority = depositor,
        associated_token::token_program = token_program
    )]
    pub depositor_share_account: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = usdc_mint,
        associated_token::authority = fund,
        associated_token::token_program = token_program
    )]
    pub vault_usdc: Box<InterfaceAccount<TokenAccount>>,

    /// CHECK: Router config PDA from the mock-swap-router program; it owns the
    /// treasury and signs the router's token CPIs
    #[account(mut)]
    pub router_config: UncheckedAccount,

    /// CHECK: Router USDC treasury ATA, owned by the router config account
    #[account(mut)]
    pub router_usdc_treasury: UncheckedAccount,

    #[account(
        constraint = *swap_router_program.address() == fund.swap_router @ FundError::InvalidSwapRouter
    )]
    /// CHECK: validated by the address constraint above
    pub swap_router_program: UncheckedAccount,

    pub associated_token_program: Program<AssociatedToken>,
    pub token_program: Interface<'static, TokenInterface>,
    pub system_program: Program<System>,
    // remaining_accounts: for each asset index 0..asset_count, in order:
    //   [asset_config, vault, asset_mint, asset_rate, price_feed]
}

/// Deposit USDC, receive shares priced at net asset value, and immediately deploy
/// the deposit into the basket at its target weights. The fund must be fully
/// allocated first: the weights sum to exactly 10000, so every deposit is fully
/// invested. For each asset the handler swaps `usdc_amount * weight_bps / 10000`
/// through the registered router, so a depositor's money is invested in the same
/// transaction they put it in (only sub-cent rounding dust can remain as USDC).
pub fn handle_deposit(
    context: &mut Context<DepositAccountConstraints>,
    usdc_amount: u64,
    minimum_shares: u64,
) -> Result<()> {
    require!(usdc_amount > 0, FundError::ZeroDeposit);
    // A fund accepts deposits only once its weights sum to 100%, so a deposit is
    // always fully invested. A half-configured or under-allocated basket is closed.
    require!(
        context.accounts.fund.total_weight_bps == 10_000,
        FundError::FundNotFullyAllocated
    );

    let total_shares = context.accounts.fund.total_shares;
    let usdc_decimals = context.accounts.usdc_mint.decimals();
    let fund_index = context.accounts.fund.index;
    let fund_bump = context.accounts.fund.bump;
    let fund_key = *context.accounts.fund.address();
    let max_slippage_bps = context.accounts.fund.max_slippage_bps;
    let asset_count = context.accounts.fund.asset_count as usize;
    // The holdings the program has accounted for, not the vaults' token
    // balances: a donation straight into a vault changes a balance and none of
    // these, so it cannot move the share price.
    let mut usdc_holdings = context.accounts.fund.usdc_holdings;
    let mut asset_holdings = context.accounts.fund.asset_holdings;

    let now = Clock::get()?.unix_timestamp;

    // Net asset value over the complete asset set. The assets are exactly indices
    // 0..asset_count, so requiring five accounts per index, in order, each with a
    // matching index, makes it impossible to omit an asset and understate NAV.
    let remaining = context.remaining_accounts()?;
    require!(
        remaining.len() == asset_count * 5,
        FundError::IncompleteAssetAccounts
    );

    let mut nav: u128 = usdc_holdings as u128;

    for index in 0..asset_count {
        let config_account = &remaining[index * 5];
        let vault_account = &remaining[index * 5 + 1];
        let feed_account = &remaining[index * 5 + 4];

        let config = AssetConfig::load_checked(config_account)?;
        require_keys_eq!(config.fund, fund_key, FundError::InvalidAssetAccount);
        require!(
            config.index as usize == index,
            FundError::InvalidAssetAccount
        );
        require_keys_eq!(
            *vault_account.address(),
            config.vault,
            FundError::InvalidAssetAccount
        );

        let price = load_price(feed_account, &config.price_feed, now)?;
        let amount = asset_holdings[index];
        nav = nav
            .checked_add(asset_value_in_usdc(
                amount as u128,
                price,
                config.decimals,
                usdc_decimals,
            )?)
            .ok_or(FundError::MathOverflow)?;
    }

    // shares = usdc_amount * total_shares / nav (floor); first deposit is 1:1.
    let shares_to_mint: u64 = if total_shares == 0 {
        usdc_amount
    } else {
        (usdc_amount as u128)
            .checked_mul(total_shares as u128)
            .ok_or(FundError::MathOverflow)?
            .checked_div(nav)
            .ok_or(FundError::MathOverflow)? as u64
    };

    require!(shares_to_mint >= minimum_shares, FundError::SlippageTooHigh);

    context.accounts.fund.total_shares = total_shares
        .checked_add(shares_to_mint)
        .ok_or(FundError::MathOverflow)?;
    usdc_holdings = usdc_holdings
        .checked_add(usdc_amount)
        .ok_or(FundError::MathOverflow)?;

    // Pull the depositor's USDC into the fund's USDC vault.
    let transfer_accounts = TransferChecked {
        from: context.accounts.depositor_usdc_account.to_cpi_handle_mut(),
        mint: context.accounts.usdc_mint.to_cpi_handle(),
        to: context.accounts.vault_usdc.to_cpi_handle_mut(),
        authority: context.accounts.depositor.cpi_handle(),
    };
    let cpi_ctx = CpiContext::new(context.accounts.token_program.address(), transfer_accounts);
    transfer_checked(cpi_ctx, usdc_amount, usdc_decimals)?;

    let index_bytes = fund_index.to_le_bytes();
    let signer_seeds: &[&[&[u8]]] = &[&[b"fund", index_bytes.as_ref(), &[fund_bump]]];

    // `fund` signs every CPI below. It is a data account holding a live
    // borrow on its buffer, which the runtime would reject when the CPI borrows
    // the same account, so hand the borrow back for the duration.
    // `release_borrow` flushes the pending writes and `reacquire_borrow_mut`
    // re-reads them.
    context.accounts.fund.release_borrow()?;

    // Deploy the deposit across the basket at its target weights. Each leg swaps a
    // weight-sized slice of the deposit through the router, under an oracle-computed
    // slippage floor. The fund PDA signs, since the USDC leaves a vault only it
    // controls.
    for index in 0..asset_count {
        let config_account = &remaining[index * 5];
        let mut vault_account = remaining[index * 5 + 1];
        let mut mint_account = remaining[index * 5 + 2];
        let rate_account = &remaining[index * 5 + 3];
        let feed_account = &remaining[index * 5 + 4];

        let config = AssetConfig::load_checked(config_account)?;
        require_keys_eq!(
            *mint_account.address(),
            config.mint,
            FundError::InvalidAssetAccount
        );

        if config.weight_bps == 0 {
            continue;
        }

        let deploy_usdc: u64 = (usdc_amount as u128)
            .checked_mul(config.weight_bps as u128)
            .ok_or(FundError::MathOverflow)?
            .checked_div(10_000)
            .ok_or(FundError::MathOverflow)? as u64;

        if deploy_usdc == 0 {
            continue;
        }

        // Slippage floor anchored to the oracle: expected_out is what deploy_usdc
        // buys at the Pyth price, allowed to fall short by at most max_slippage_bps.
        let price = load_price(feed_account, &config.price_feed, now)?;
        let expected_out =
            usdc_to_asset_amount(deploy_usdc as u128, price, config.decimals, usdc_decimals)?;
        let minimum_asset_out: u64 = expected_out
            .checked_mul((10_000 - max_slippage_bps) as u128)
            .ok_or(FundError::MathOverflow)?
            .checked_div(10_000)
            .ok_or(FundError::MathOverflow)?
            .try_into()
            .map_err(|_| FundError::MathOverflow)?;

        // Record what the swap actually moves, measured on the vaults, rather
        // than what was asked for.
        let asset_before = read_token_amount(&vault_account)?;
        let usdc_before = context.accounts.vault_usdc.amount();

        let cpi_accounts = RouterSwapAccounts {
            caller: context.accounts.fund.to_cpi_handle(),
            router_config: context.accounts.router_config.cpi_handle(),
            asset_rate: CpiHandle::readonly(rate_account),
            usdc_mint: context.accounts.usdc_mint.to_cpi_handle(),
            asset_mint: CpiHandleMut::writable(&mut mint_account),
            caller_usdc_account: context.accounts.vault_usdc.to_cpi_handle_mut(),
            caller_asset_account: CpiHandleMut::writable(&mut vault_account),
            router_usdc_treasury: context.accounts.router_usdc_treasury.cpi_handle_mut(),
            associated_token_program: context.accounts.associated_token_program.cpi_handle(),
            token_program: context.accounts.token_program.cpi_handle(),
            system_program: context.accounts.system_program.cpi_handle(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            context.accounts.swap_router_program.address(),
            cpi_accounts,
            signer_seeds,
        );
        mock_swap_router::cpi::swap_usdc_for_asset(cpi_ctx, deploy_usdc, minimum_asset_out)?;

        let asset_received = read_token_amount(&vault_account)?
            .checked_sub(asset_before)
            .ok_or(FundError::MathOverflow)?;
        let usdc_spent = usdc_before
            .checked_sub(context.accounts.vault_usdc.amount())
            .ok_or(FundError::MathOverflow)?;
        // A leg that spends USDC and buys nothing would leave shares minted
        // against no recorded value, and every later deposit would divide by a
        // zero NAV. Refuse it: the deposit is too small for this basket.
        require!(asset_received > 0, FundError::DepositTooSmall);
        asset_holdings[index] = asset_holdings[index]
            .checked_add(asset_received)
            .ok_or(FundError::MathOverflow)?;
        usdc_holdings = usdc_holdings
            .checked_sub(usdc_spent)
            .ok_or(FundError::MathOverflow)?;
    }

    // Mint the shares last, with the fund PDA signing as the share mint authority.
    let mint_accounts = MintTo {
        mint: context.accounts.share_mint.to_cpi_handle_mut(),
        to: context.accounts.depositor_share_account.to_cpi_handle_mut(),
        authority: context.accounts.fund.to_cpi_handle(),
    };
    let cpi_ctx = CpiContext::new_with_signer(
        context.accounts.token_program.address(),
        mint_accounts,
        signer_seeds,
    );
    mint_to(cpi_ctx, shares_to_mint)?;

    context.accounts.fund.reacquire_borrow_mut()?;
    context.accounts.fund.usdc_holdings = usdc_holdings;
    context.accounts.fund.asset_holdings = asset_holdings;

    Ok(())
}
