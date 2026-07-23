//! quasar-test integration tests for the mock swap router: initialize it, set
//! a rate (which also creates the USDC treasury), then swap USDC for an asset,
//! asserting the minted amounts and treasury flows.

use {
    crate::{
        cpi::{InitializeRouterInstruction, SetRateInstruction, SwapUsdcForAssetInstruction},
        state::{AssetRate, RouterAuthorityPda, TreasuryPda},
    },
    quasar_test::prelude::*,
};

const DECIMALS: u8 = 6;
const RATE: u64 = 250; // 250 USDC base units per asset base unit

// Deterministic addresses.
const AUTHORITY: Pubkey = Pubkey::new_from_array([1; 32]);
const USDC_MINT: Pubkey = Pubkey::new_from_array([2; 32]);
const ASSET_MINT: Pubkey = Pubkey::new_from_array([3; 32]);
const CALLER_USDC: Pubkey = Pubkey::new_from_array([4; 32]);
const CALLER_ASSET: Pubkey = Pubkey::new_from_array([5; 32]);

#[quasar_test]
fn initialize_and_swap_usdc_for_asset(test: &mut Test) {
    let router_authority = test.derive_pda(RouterAuthorityPda::seeds());
    let treasury = test.derive_pda(TreasuryPda::seeds());
    let rate = test.derive_pda(AssetRate::seeds(&ASSET_MINT));

    const USDC_IN: u64 = 1_000;
    const ASSET_OUT: u64 = USDC_IN / RATE; // 4

    test.add(Wallet::new().at(AUTHORITY));
    test.add(Mint::new(AUTHORITY).at(USDC_MINT).decimals(DECIMALS));
    // The asset mint's authority is the router-authority PDA, so the router
    // can mint it.
    test.add(Mint::new(router_authority).at(ASSET_MINT).decimals(DECIMALS));
    test.add(
        TokenAccount::new(USDC_MINT, AUTHORITY)
            .at(CALLER_USDC)
            .amount(USDC_IN),
    );
    test.add(TokenAccount::new(ASSET_MINT, AUTHORITY).at(CALLER_ASSET));

    test.send(InitializeRouterInstruction {
        authority: AUTHORITY,
        usdc_mint: USDC_MINT,
    })
    .succeeds();
    test.send(SetRateInstruction {
        authority: AUTHORITY,
        asset_mint: ASSET_MINT,
        usdc_mint: USDC_MINT,
        usdc_per_token: RATE,
    })
    .succeeds();
    test.send(SwapUsdcForAssetInstruction {
        caller: AUTHORITY,
        asset_rate: rate,
        usdc_mint: USDC_MINT,
        asset_mint: ASSET_MINT,
        caller_usdc_account: CALLER_USDC,
        caller_asset_account: CALLER_ASSET,
        usdc_amount_in: USDC_IN,
        minimum_asset_out: ASSET_OUT,
    })
    .succeeds()
    // Caller paid all USDC and received the minted asset; the treasury holds
    // the USDC.
    .has_tokens(CALLER_USDC, 0)
    .has_tokens(CALLER_ASSET, ASSET_OUT)
    .has_tokens(treasury, USDC_IN);
}
