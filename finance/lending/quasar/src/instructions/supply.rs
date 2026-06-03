use {
    crate::{
        error::LendingError,
        logic::{accrue, now, snapshot_reserve},
        math::{mul_div_floor, total_liquidity},
        state::Reserve,
    },
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

/// Reserve PDA signer seeds, used to authorize mint/transfer from the vault.
macro_rules! reserve_seeds {
    ($lending_market:expr, $liquidity_mint:expr, $bump:expr) => {
        [
            Seed::from(crate::constants::RESERVE_SEED),
            Seed::from($lending_market.as_ref()),
            Seed::from($liquidity_mint.as_ref()),
            Seed::from($bump.as_ref()),
        ]
    };
}
pub(crate) use reserve_seeds;

// ---------------------------------------------------------------------------
// deposit_reserve_liquidity
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct DepositReserveLiquidity {
    #[account(mut)]
    pub supplier: Signer,
    #[account(mut, has_one(liquidity_mint), has_one(liquidity_vault), has_one(share_mint))]
    pub reserve: Account<Reserve>,
    pub liquidity_mint: Account<Mint>,
    #[account(mut)]
    pub liquidity_vault: Account<Token>,
    #[account(mut)]
    pub share_mint: Account<Mint>,
    #[account(mut)]
    pub supplier_liquidity: Account<Token>,
    #[account(mut)]
    pub supplier_share: Account<Token>,
    pub token_program: Program<TokenProgram>,
}

impl DepositReserveLiquidity {
    #[inline(always)]
    pub fn run(&mut self, amount: u64) -> Result<(), ProgramError> {
        require!(amount > 0, LendingError::ZeroAmount);
        let slot = now()?;

        let mut reserve = snapshot_reserve(&self.reserve);
        accrue(&mut reserve, slot)?;

        let total = total_liquidity(
            reserve.available_liquidity,
            reserve.borrowed_amount_scaled,
            reserve.cumulative_borrow_rate_index,
        )?;
        let shares = if reserve.share_mint_supply == 0 {
            amount as u128
        } else {
            mul_div_floor(amount as u128, reserve.share_mint_supply as u128, total)?
        };
        require!(shares > 0, LendingError::DepositTooSmall);
        let shares = u64::try_from(shares).map_err(|_| LendingError::MathOverflow)?;

        reserve.available_liquidity = reserve
            .available_liquidity
            .checked_add(amount)
            .ok_or(LendingError::MathOverflow)?;
        reserve.share_mint_supply = reserve
            .share_mint_supply
            .checked_add(shares)
            .ok_or(LendingError::MathOverflow)?;

        let decimals = reserve.liquidity_decimals;
        let bump = [reserve.bump];
        let lending_market = reserve.lending_market;
        let liquidity_mint = reserve.liquidity_mint;
        self.reserve.set_inner(reserve);

        self.token_program
            .transfer_checked(
                &self.supplier_liquidity,
                &self.liquidity_mint,
                &self.liquidity_vault,
                &self.supplier,
                amount,
                decimals,
            )
            .invoke()?;

        let seeds = reserve_seeds!(lending_market, liquidity_mint, bump);
        self.token_program
            .mint_to(&self.share_mint, &self.supplier_share, &self.reserve, shares)
            .invoke_signed(&seeds)
    }
}

// ---------------------------------------------------------------------------
// redeem_reserve_collateral
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct RedeemReserveCollateral {
    #[account(mut)]
    pub supplier: Signer,
    #[account(mut, has_one(liquidity_mint), has_one(liquidity_vault), has_one(share_mint))]
    pub reserve: Account<Reserve>,
    pub liquidity_mint: Account<Mint>,
    #[account(mut)]
    pub liquidity_vault: Account<Token>,
    #[account(mut)]
    pub share_mint: Account<Mint>,
    #[account(mut)]
    pub supplier_liquidity: Account<Token>,
    #[account(mut)]
    pub supplier_share: Account<Token>,
    pub token_program: Program<TokenProgram>,
}

impl RedeemReserveCollateral {
    #[inline(always)]
    pub fn run(&mut self, shares: u64) -> Result<(), ProgramError> {
        require!(shares > 0, LendingError::ZeroAmount);
        let slot = now()?;

        let mut reserve = snapshot_reserve(&self.reserve);
        accrue(&mut reserve, slot)?;
        require!(reserve.share_mint_supply > 0, LendingError::InsufficientLiquidity);

        let total = total_liquidity(
            reserve.available_liquidity,
            reserve.borrowed_amount_scaled,
            reserve.cumulative_borrow_rate_index,
        )?;
        let liquidity = mul_div_floor(shares as u128, total, reserve.share_mint_supply as u128)?;
        let liquidity = u64::try_from(liquidity).map_err(|_| LendingError::MathOverflow)?;
        require!(
            liquidity <= reserve.available_liquidity,
            LendingError::InsufficientLiquidity
        );

        reserve.available_liquidity = reserve
            .available_liquidity
            .checked_sub(liquidity)
            .ok_or(LendingError::MathOverflow)?;
        reserve.share_mint_supply = reserve
            .share_mint_supply
            .checked_sub(shares)
            .ok_or(LendingError::MathOverflow)?;

        let decimals = reserve.liquidity_decimals;
        let bump = [reserve.bump];
        let lending_market = reserve.lending_market;
        let liquidity_mint = reserve.liquidity_mint;
        self.reserve.set_inner(reserve);

        self.token_program
            .burn(&self.supplier_share, &self.share_mint, &self.supplier, shares)
            .invoke()?;

        let seeds = reserve_seeds!(lending_market, liquidity_mint, bump);
        self.token_program
            .transfer_checked(
                &self.liquidity_vault,
                &self.liquidity_mint,
                &self.supplier_liquidity,
                &self.reserve,
                liquidity,
                decimals,
            )
            .invoke_signed(&seeds)
    }
}
