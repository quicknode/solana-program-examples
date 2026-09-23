use {
    crate::{
        constants::MINIMUM_SHARES,
        error::LendingError,
        logic::{accrue, now, snapshot_reserve},
        math::{mul_div_floor, net_total_liquidity, total_shares},
        state::Reserve,
    },
    quasar_lang::{cpi::Seed, prelude::*},
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
    #[account(
        mut,
        has_one(liquidity_mint),
        has_one(liquidity_vault),
        has_one(share_mint)
    )]
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
        let (slot, timestamp) = now()?;

        let mut reserve = snapshot_reserve(&self.reserve);
        accrue(&mut reserve, slot, timestamp)?;

        let total = net_total_liquidity(
            reserve.available_liquidity,
            reserve.borrowed_principal,
            reserve.borrow_accumulation_factor,
            reserve.accumulated_protocol_fees,
        )?;
        let shares = if reserve.share_mint_supply == 0 && total == 0 {
            // Bootstrap: shares track liquidity one-for-one, less the withheld
            // minimum, so the share supply can never start at a dust amount.
            amount
                .checked_sub(MINIMUM_SHARES)
                .ok_or(LendingError::DepositTooSmall)? as u128
        } else {
            // The withheld minimum counts as shares nobody holds, here and in
            // every other conversion, so its slice of the pool is locked for
            // good. That is what stops share inflation: a lone supplier who
            // borrows from their own reserve can lift the total with the
            // interest they owe, and deposits and redemptions that round in the
            // pool's favour lift it further, until one share is worth enough to
            // round a later deposit down. With the minimum counted, their one
            // share is 1 of 1_001 and whatever they leave in the pool goes
            // mostly to shares nobody redeems.
            //
            // A reserve whose suppliers have all left takes this branch too:
            // the minimum's slice is still in the total, so the next deposit is
            // priced against it rather than bootstrapped.
            mul_div_floor(
                amount as u128,
                total_shares(reserve.share_mint_supply)?,
                total,
            )?
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
            .mint_to(
                &self.share_mint,
                &self.supplier_share,
                &self.reserve,
                shares,
            )
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
    #[account(
        mut,
        has_one(liquidity_mint),
        has_one(liquidity_vault),
        has_one(share_mint)
    )]
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
        let (slot, timestamp) = now()?;

        let mut reserve = snapshot_reserve(&self.reserve);
        accrue(&mut reserve, slot, timestamp)?;
        require!(
            reserve.share_mint_supply > 0,
            LendingError::InsufficientLiquidity
        );

        let total = net_total_liquidity(
            reserve.available_liquidity,
            reserve.borrowed_principal,
            reserve.borrow_accumulation_factor,
            reserve.accumulated_protocol_fees,
        )?;
        // The withheld minimum counts as shares nobody holds, as it does in
        // deposit_reserve_liquidity, so its slice of the pool never leaves.
        let liquidity = mul_div_floor(
            shares as u128,
            total,
            total_shares(reserve.share_mint_supply)?,
        )?;
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
            .burn(
                &self.supplier_share,
                &self.share_mint,
                &self.supplier,
                shares,
            )
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
