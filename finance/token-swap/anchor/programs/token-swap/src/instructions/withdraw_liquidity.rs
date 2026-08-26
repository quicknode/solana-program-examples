use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{self, Burn, Mint, TokenAccount, TokenInterface, TransferChecked},
};

use crate::{
    constants::{AUTHORITY_SEED, CONFIG_SEED, LIQUIDITY_SEED, MINIMUM_LIQUIDITY},
    errors::AmmError,
    state::{Config, PoolConfig},
};

pub fn handle_withdraw_liquidity(
    context: &mut Context<WithdrawLiquidityAccountConstraints>,
    amount: u64,
    minimum_token_a_out: u64,
    minimum_token_b_out: u64,
) -> Result<()> {
    let authority_bump = context.bumps.pool_authority;
    let authority_seeds = &[
        &context.accounts.pool_config.config.to_bytes(),
        &context.accounts.mint_a.address().to_bytes(),
        &context.accounts.mint_b.address().to_bytes(),
        AUTHORITY_SEED,
        &[authority_bump],
    ];
    let signer_seeds = &[&authority_seeds[..]];

    // LPs withdraw a proportional share of the *effective* reserves
    // (vault balance minus the admin's accumulated fee claim). The admin's
    // owed slice physically remains in the vaults but is not distributed to
    // exiting LPs - it's claimed separately via `claim_admin_fees`.
    let pool_config = &context.accounts.pool_config;
    // checked_sub: admin_fees_owed is an invariant subset of the vault balance;
    // a raw `-` would wrap silently on a BPF release build if that ever broke.
    let effective_pool_a = context
        .accounts
        .pool_a
        .amount()
        .checked_sub(pool_config.admin_fees_owed_a)
        .ok_or(AmmError::MathOverflow)?;
    let effective_pool_b = context
        .accounts
        .pool_b
        .amount()
        .checked_sub(pool_config.admin_fees_owed_b)
        .ok_or(AmmError::MathOverflow)?;

    // Proportional-withdraw formula:
    //   amount_out = lp_amount * effective_reserve / (lp_supply + MINIMUM_LIQUIDITY)
    // The `+ MINIMUM_LIQUIDITY` accounts for the bootstrap floor that was
    // locked away on the first deposit and is *not* part of the LP supply
    // counter (mint::supply doesn't include it) but *is* part of the
    // reserves - so the divisor needs the same adjustment to keep shares
    // honest.
    //
    // u128 + checked: `lp_amount * reserve` can fill the full u128 (both
    // factors are u64). Multiply before divide to preserve precision; floor
    // is protocol-favouring (sub-base-unit rounding stays with the pool,
    // grows LP value for everyone still in).
    //
    // Both amounts are computed up-front (before the slippage checks) so
    // the LP gets a consistent error regardless of which side trips first,
    // and so we don't transfer one side then revert.
    let divisor = (context.accounts.liquidity_provider_mint.supply() as u128)
        .checked_add(MINIMUM_LIQUIDITY as u128)
        .ok_or(AmmError::MathOverflow)?;
    let amount_a_u128 = (amount as u128)
        .checked_mul(effective_pool_a as u128)
        .ok_or(AmmError::MathOverflow)?
        .checked_div(divisor)
        .ok_or(AmmError::MathOverflow)?;
    let amount_a: u64 = u64::try_from(amount_a_u128).map_err(|_| AmmError::MathOverflow)?;
    let amount_b_u128 = (amount as u128)
        .checked_mul(effective_pool_b as u128)
        .ok_or(AmmError::MathOverflow)?
        .checked_div(divisor)
        .ok_or(AmmError::MathOverflow)?;
    let amount_b: u64 = u64::try_from(amount_b_u128).map_err(|_| AmmError::MathOverflow)?;

    // LP's slippage protection: if the pool ratio shifted between the LP
    // quoting their exit and this tx landing (e.g. a big swap drained one
    // side), the proportional share comes back with a different mix than
    // expected. Revert so the LP can bail / requote.
    require!(
        amount_a >= minimum_token_a_out,
        AmmError::WithdrawalBelowMinimum
    );
    require!(
        amount_b >= minimum_token_b_out,
        AmmError::WithdrawalBelowMinimum
    );

    // transfer_checked verifies the mint + decimals at the token program.
    token_interface::transfer_checked(
        CpiContext::new_with_signer(
            context.accounts.token_program.address(),
            TransferChecked {
                from: context.accounts.pool_a.to_cpi_handle_mut(),
                mint: context.accounts.mint_a.to_cpi_handle(),
                to: context.accounts.token_a.to_cpi_handle_mut(),
                authority: context.accounts.pool_authority.cpi_handle(),
            },
            signer_seeds,
        ),
        amount_a,
        context.accounts.mint_a.decimals(),
    )?;

    token_interface::transfer_checked(
        CpiContext::new_with_signer(
            context.accounts.token_program.address(),
            TransferChecked {
                from: context.accounts.pool_b.to_cpi_handle_mut(),
                mint: context.accounts.mint_b.to_cpi_handle(),
                to: context.accounts.token_b.to_cpi_handle_mut(),
                authority: context.accounts.pool_authority.cpi_handle(),
            },
            signer_seeds,
        ),
        amount_b,
        context.accounts.mint_b.decimals(),
    )?;

    // Burn the liquidity tokens
    // It will fail if the amount is invalid
    token_interface::burn(
        CpiContext::new(
            context.accounts.token_program.address(),
            Burn {
                mint: context.accounts.liquidity_provider_mint.to_cpi_handle_mut(),
                from: context
                    .accounts
                    .liquidity_provider_token
                    .to_cpi_handle_mut(),
                authority: context.accounts.withdrawer.cpi_handle(),
            },
        ),
        amount,
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct WithdrawLiquidityAccountConstraints {
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
    )]
    pub config: BorshAccount<Config>,

    #[account(
        seeds = [
            pool_config.config.as_ref(),
            pool_config.mint_a.as_ref(),
            pool_config.mint_b.as_ref(),
        ],
        bump = pool_config.bump,
    )]
    pub pool_config: BorshAccount<PoolConfig>,

    /// CHECK: Read only authority
    #[account(
        seeds = [
            pool_config.config.as_ref(),
            mint_a.address().as_ref(),
            mint_b.address().as_ref(),
            AUTHORITY_SEED,
        ],
        bump,
    )]
    pub pool_authority: UncheckedAccount,

    pub withdrawer: Signer,

    #[account(
        mut,
        seeds = [
            pool_config.config.as_ref(),
            mint_a.address().as_ref(),
            mint_b.address().as_ref(),
            LIQUIDITY_SEED,
        ],
        bump,
    )]
    pub liquidity_provider_mint: Box<InterfaceAccount<Mint>>,

    #[account(mut, address = pool_config.mint_a)]
    pub mint_a: Box<InterfaceAccount<Mint>>,

    #[account(mut, address = pool_config.mint_b)]
    pub mint_b: Box<InterfaceAccount<Mint>>,

    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = pool_authority,
        associated_token::token_program = token_program,
    )]
    pub pool_a: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = mint_b,
        associated_token::authority = pool_authority,
        associated_token::token_program = token_program,
    )]
    pub pool_b: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = liquidity_provider_mint,
        associated_token::authority = withdrawer,
        associated_token::token_program = token_program,
    )]
    pub liquidity_provider_token: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint_a,
        associated_token::authority = withdrawer,
        associated_token::token_program = token_program,
    )]
    pub token_a: Box<InterfaceAccount<TokenAccount>>,

    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint_b,
        associated_token::authority = withdrawer,
        associated_token::token_program = token_program,
    )]
    pub token_b: Box<InterfaceAccount<TokenAccount>>,

    /// The account paying for all rents
    #[account(mut)]
    pub payer: Signer,

    /// Solana ecosystem accounts
    pub token_program: Interface<'static, TokenInterface>,
    pub associated_token_program: Program<AssociatedToken>,
    pub system_program: Program<System>,
}
