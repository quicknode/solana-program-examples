use std::str::FromStr;

use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::spl_token_2022::{
        extension::{BaseStateWithExtensions, StateWithExtensions},
        state::Mint,
    },
    token_interface::spl_token_metadata_interface::state::TokenMetadata,
};

use crate::{ABListError, ABWallet, Mode};

#[derive(Accounts)]
pub struct TxHookAccountConstraints {
    /// CHECK:
    pub source_token_account: UncheckedAccount,
    /// CHECK:
    pub mint: UncheckedAccount,
    /// CHECK:
    pub destination_token_account: UncheckedAccount,
    /// CHECK:
    pub owner_delegate: UncheckedAccount,
    /// CHECK:
    pub meta_list: UncheckedAccount,
    /// CHECK:
    pub ab_wallet: UncheckedAccount,
}

impl TxHookAccountConstraints {
    pub fn tx_hook(&self, amount: u64) -> Result<()> {
        let mint_data = self.mint.account().try_borrow()?;
        let mint = StateWithExtensions::<Mint>::unpack(&mint_data)?;

        let metadata = mint.get_variable_len_extension::<TokenMetadata>()?;
        let decoded_mode = Self::decode_metadata(&metadata)?;
        let decoded_wallet_mode = self.decode_wallet_mode()?;

        match (decoded_mode, decoded_wallet_mode) {
            // first check the force allow modes
            (DecodedMintMode::Allow, DecodedWalletMode::Allow) => Ok(()),
            (DecodedMintMode::Allow, _) => Err(ABListError::WalletNotAllowed.into()),
            // then check if the wallet is blocked
            (_, DecodedWalletMode::Block) => Err(ABListError::WalletBlocked.into()),
            (DecodedMintMode::Block, _) => Ok(()),
            // lastly check the threshold mode
            (DecodedMintMode::Threshold(threshold), DecodedWalletMode::None)
                if amount >= threshold =>
            {
                Err(ABListError::AmountNotAllowed.into())
            }
            (DecodedMintMode::Threshold(_), _) => Ok(()),
        }
    }

    fn decode_wallet_mode(&self) -> Result<DecodedWalletMode> {
        let wallet_data = self.ab_wallet.account().try_borrow()?;
        if wallet_data.is_empty() {
            return Ok(DecodedWalletMode::None);
        }

        // v2 has no `Account::try_from`, so the discriminator is checked and
        // the payload read by hand.
        let disc_len = <ABWallet as anchor_lang::Discriminator>::DISCRIMINATOR.len();
        require!(
            wallet_data.len() > disc_len
                && &wallet_data[..disc_len]
                    == <ABWallet as anchor_lang::Discriminator>::DISCRIMINATOR,
            ABListError::InvalidMetadata
        );
        let mut payload = &wallet_data[disc_len..];
        let wallet = <ABWallet as wincode::SchemaRead<anchor_lang::BorshConfig>>::get(&mut payload)
            .map_err(|_| ABListError::InvalidMetadata)?;

        if wallet.allowed {
            Ok(DecodedWalletMode::Allow)
        } else {
            Ok(DecodedWalletMode::Block)
        }
    }

    fn decode_metadata(metadata: &TokenMetadata) -> Result<DecodedMintMode> {
        let mut mode = Mode::Allow;
        let mut threshold = 0;

        for (key, value) in metadata.additional_metadata.iter() {
            if key == "AB" {
                mode = Mode::from_str(value).map_err(|_| ABListError::InvalidMetadata)?;
                if mode == Mode::Allow {
                    return Ok(DecodedMintMode::Allow);
                } else if mode == Mode::Block {
                    return Ok(DecodedMintMode::Block);
                } else if mode == Mode::Mixed && threshold > 0 {
                    return Ok(DecodedMintMode::Threshold(threshold));
                }
            } else if key == "threshold" {
                threshold = u64::from_str(value).map_err(|_| ABListError::InvalidMetadata)?;
                if threshold > 0 {
                    return Ok(DecodedMintMode::Threshold(threshold));
                }
            }
        }

        // we have early returns above, but we can reach here if metadata is meddled with
        // which is why we have this fallback
        // also, anchor doesn't yet support removing keys from metadata, which means that if we set threshold, we can never remove the KV pair
        // only set it to 0

        if mode == Mode::Allow {
            return Ok(DecodedMintMode::Allow);
        } else if mode == Mode::Block {
            return Ok(DecodedMintMode::Block);
        }

        Ok(DecodedMintMode::Threshold(threshold))
    }
}

enum DecodedMintMode {
    Allow,
    Block,
    Threshold(u64),
}

enum DecodedWalletMode {
    Allow,
    Block,
    None,
}
