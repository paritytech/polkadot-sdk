// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Types used to connect to the Rialto-Substrate chain.

use bp_messages::MessageNonce;
use codec::Encode;
use frame_support::weights::Weight;
use relay_substrate_client::{
	Chain, ChainBase, ChainWithBalances, ChainWithMessages, ChainWithTransactions,
	Error as SubstrateError, SignParam, UnsignedTransaction,
};
use sp_core::{storage::StorageKey, Pair};
use sp_runtime::{generic::SignedPayload, traits::IdentifyAccount};
use std::time::Duration;

/// Rialto header id.
pub type HeaderId =
	relay_utils::HeaderId<rialto_parachain_runtime::Hash, rialto_parachain_runtime::BlockNumber>;

/// Rialto parachain definition
#[derive(Debug, Clone, Copy)]
pub struct RialtoParachain;

impl ChainBase for RialtoParachain {
	type BlockNumber = rialto_parachain_runtime::BlockNumber;
	type Hash = rialto_parachain_runtime::Hash;
	type Hasher = rialto_parachain_runtime::Hashing;
	type Header = rialto_parachain_runtime::Header;

	type AccountId = rialto_parachain_runtime::AccountId;
	type Balance = rialto_parachain_runtime::Balance;
	type Index = rialto_parachain_runtime::Index;
	type Signature = rialto_parachain_runtime::Signature;

	fn max_extrinsic_size() -> u32 {
		bp_rialto_parachain::RialtoParachain::max_extrinsic_size()
	}

	fn max_extrinsic_weight() -> Weight {
		bp_rialto_parachain::RialtoParachain::max_extrinsic_weight()
	}
}

impl Chain for RialtoParachain {
	const NAME: &'static str = "RialtoParachain";
	// RialtoParachain token has no value, but we associate it with DOT token
	const TOKEN_ID: Option<&'static str> = Some("polkadot");
	const BEST_FINALIZED_HEADER_ID_METHOD: &'static str =
		bp_rialto_parachain::BEST_FINALIZED_RIALTO_PARACHAIN_HEADER_METHOD;
	const AVERAGE_BLOCK_INTERVAL: Duration = Duration::from_secs(5);
	const STORAGE_PROOF_OVERHEAD: u32 = bp_rialto_parachain::EXTRA_STORAGE_PROOF_SIZE;

	type SignedBlock = rialto_parachain_runtime::SignedBlock;
	type Call = rialto_parachain_runtime::RuntimeCall;
}

impl ChainWithBalances for RialtoParachain {
	fn account_info_storage_key(account_id: &Self::AccountId) -> StorageKey {
		use frame_support::storage::generator::StorageMap;
		StorageKey(
			frame_system::Account::<rialto_parachain_runtime::Runtime>::storage_map_final_key(
				account_id,
			),
		)
	}
}

impl ChainWithMessages for RialtoParachain {
	const WITH_CHAIN_MESSAGES_PALLET_NAME: &'static str =
		bp_rialto_parachain::WITH_RIALTO_PARACHAIN_MESSAGES_PALLET_NAME;
	const TO_CHAIN_MESSAGE_DETAILS_METHOD: &'static str =
		bp_rialto_parachain::TO_RIALTO_PARACHAIN_MESSAGE_DETAILS_METHOD;
	const FROM_CHAIN_MESSAGE_DETAILS_METHOD: &'static str =
		bp_rialto_parachain::FROM_RIALTO_PARACHAIN_MESSAGE_DETAILS_METHOD;
	const PAY_INBOUND_DISPATCH_FEE_WEIGHT_AT_CHAIN: Weight =
		bp_rialto_parachain::PAY_INBOUND_DISPATCH_FEE_WEIGHT;
	const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce =
		bp_rialto_parachain::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX;
	const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce =
		bp_rialto_parachain::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX;
	type WeightToFee = bp_rialto_parachain::WeightToFee;
	type WeightInfo = ();
}

impl ChainWithTransactions for RialtoParachain {
	type AccountKeyPair = sp_core::sr25519::Pair;
	type SignedTransaction = rialto_parachain_runtime::UncheckedExtrinsic;

	fn sign_transaction(
		param: SignParam<Self>,
		unsigned: UnsignedTransaction<Self>,
	) -> Result<Self::SignedTransaction, SubstrateError> {
		let raw_payload = SignedPayload::from_raw(
			unsigned.call,
			(
				frame_system::CheckNonZeroSender::<rialto_parachain_runtime::Runtime>::new(),
				frame_system::CheckSpecVersion::<rialto_parachain_runtime::Runtime>::new(),
				frame_system::CheckTxVersion::<rialto_parachain_runtime::Runtime>::new(),
				frame_system::CheckGenesis::<rialto_parachain_runtime::Runtime>::new(),
				frame_system::CheckEra::<rialto_parachain_runtime::Runtime>::from(
					unsigned.era.frame_era(),
				),
				frame_system::CheckNonce::<rialto_parachain_runtime::Runtime>::from(unsigned.nonce),
				frame_system::CheckWeight::<rialto_parachain_runtime::Runtime>::new(),
				pallet_transaction_payment::ChargeTransactionPayment::<
					rialto_parachain_runtime::Runtime,
				>::from(unsigned.tip),
			),
			(
				(),
				param.spec_version,
				param.transaction_version,
				param.genesis_hash,
				unsigned.era.signed_payload(param.genesis_hash),
				(),
				(),
				(),
			),
		);
		let signature = raw_payload.using_encoded(|payload| param.signer.sign(payload));
		let signer: sp_runtime::MultiSigner = param.signer.public().into();
		let (call, extra, _) = raw_payload.deconstruct();

		Ok(rialto_parachain_runtime::UncheckedExtrinsic::new_signed(
			call.into_decoded()?,
			signer.into_account().into(),
			signature.into(),
			extra,
		))
	}

	fn is_signed(tx: &Self::SignedTransaction) -> bool {
		tx.signature.is_some()
	}

	fn is_signed_by(signer: &Self::AccountKeyPair, tx: &Self::SignedTransaction) -> bool {
		tx.signature
			.as_ref()
			.map(|(address, _, _)| {
				*address == rialto_parachain_runtime::Address::Id(signer.public().into())
			})
			.unwrap_or(false)
	}

	fn parse_transaction(_tx: Self::SignedTransaction) -> Option<UnsignedTransaction<Self>> {
		unimplemented!("TODO")
	}
}

/// RialtoParachain signing params.
pub type SigningParams = sp_core::sr25519::Pair;

/// RialtoParachain header type used in headers sync.
pub type SyncHeader = relay_substrate_client::SyncHeader<rialto_parachain_runtime::Header>;
