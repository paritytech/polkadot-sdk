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
use bp_rialto::RIALTO_SYNCED_HEADERS_GRANDPA_INFO_METHOD;
use bp_runtime::ChainId;
use codec::{Compact, Decode, Encode};
use relay_substrate_client::{
	BalanceOf, Chain, ChainWithBalances, ChainWithGrandpa, ChainWithMessages,
	ChainWithTransactions, Error as SubstrateError, NonceOf, RelayChain, SignParam,
	UnderlyingChainProvider, UnsignedTransaction,
};
use sp_core::{storage::StorageKey, Pair};
use sp_runtime::{generic::SignedPayload, traits::IdentifyAccount};
use sp_session::MembershipProof;
use std::time::Duration;

/// Rialto header id.
pub type HeaderId = relay_utils::HeaderId<rialto_runtime::Hash, rialto_runtime::BlockNumber>;

/// Rialto chain definition
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rialto;

impl UnderlyingChainProvider for Rialto {
	type Chain = bp_rialto::Rialto;
}

impl Chain for Rialto {
	const ID: ChainId = bp_runtime::RIALTO_CHAIN_ID;
	const NAME: &'static str = "Rialto";
	const BEST_FINALIZED_HEADER_ID_METHOD: &'static str =
		bp_rialto::BEST_FINALIZED_RIALTO_HEADER_METHOD;
	const AVERAGE_BLOCK_INTERVAL: Duration = Duration::from_secs(5);

	type SignedBlock = rialto_runtime::SignedBlock;
	type Call = rialto_runtime::RuntimeCall;
}

impl ChainWithGrandpa for Rialto {
	const SYNCED_HEADERS_GRANDPA_INFO_METHOD: &'static str =
		RIALTO_SYNCED_HEADERS_GRANDPA_INFO_METHOD;

	type KeyOwnerProof = MembershipProof;
}

impl RelayChain for Rialto {
	const PARAS_PALLET_NAME: &'static str = bp_rialto::PARAS_PALLET_NAME;
	const PARACHAINS_FINALITY_PALLET_NAME: &'static str =
		bp_rialto::WITH_RIALTO_BRIDGE_PARAS_PALLET_NAME;
}

impl ChainWithMessages for Rialto {
	const WITH_CHAIN_MESSAGES_PALLET_NAME: &'static str =
		bp_rialto::WITH_RIALTO_MESSAGES_PALLET_NAME;
	// TODO (https://github.com/paritytech/parity-bridges-common/issues/1692): change the name
	const WITH_CHAIN_RELAYERS_PALLET_NAME: Option<&'static str> = Some("BridgeRelayers");
	const TO_CHAIN_MESSAGE_DETAILS_METHOD: &'static str =
		bp_rialto::TO_RIALTO_MESSAGE_DETAILS_METHOD;
	const FROM_CHAIN_MESSAGE_DETAILS_METHOD: &'static str =
		bp_rialto::FROM_RIALTO_MESSAGE_DETAILS_METHOD;
	const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce =
		bp_rialto::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX;
	const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce =
		bp_rialto::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX;
}

impl ChainWithBalances for Rialto {
	fn account_info_storage_key(account_id: &Self::AccountId) -> StorageKey {
		use frame_support::storage::generator::StorageMap;
		StorageKey(frame_system::Account::<rialto_runtime::Runtime>::storage_map_final_key(
			account_id,
		))
	}
}

impl ChainWithTransactions for Rialto {
	type AccountKeyPair = sp_core::sr25519::Pair;
	type SignedTransaction = rialto_runtime::UncheckedExtrinsic;

	fn sign_transaction(
		param: SignParam<Self>,
		unsigned: UnsignedTransaction<Self>,
	) -> Result<Self::SignedTransaction, SubstrateError> {
		let raw_payload = SignedPayload::from_raw(
			unsigned.call.clone(),
			(
				frame_system::CheckNonZeroSender::<rialto_runtime::Runtime>::new(),
				frame_system::CheckSpecVersion::<rialto_runtime::Runtime>::new(),
				frame_system::CheckTxVersion::<rialto_runtime::Runtime>::new(),
				frame_system::CheckGenesis::<rialto_runtime::Runtime>::new(),
				frame_system::CheckEra::<rialto_runtime::Runtime>::from(unsigned.era.frame_era()),
				frame_system::CheckNonce::<rialto_runtime::Runtime>::from(unsigned.nonce),
				frame_system::CheckWeight::<rialto_runtime::Runtime>::new(),
				pallet_transaction_payment::ChargeTransactionPayment::<rialto_runtime::Runtime>::from(unsigned.tip),
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

		Ok(rialto_runtime::UncheckedExtrinsic::new_signed(
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
			.map(|(address, _, _)| *address == rialto_runtime::Address::Id(signer.public().into()))
			.unwrap_or(false)
	}

	fn parse_transaction(tx: Self::SignedTransaction) -> Option<UnsignedTransaction<Self>> {
		let extra = &tx.signature.as_ref()?.2;
		Some(
			UnsignedTransaction::new(
				tx.function.into(),
				Compact::<NonceOf<Self>>::decode(&mut &extra.5.encode()[..]).ok()?.into(),
			)
			.tip(Compact::<BalanceOf<Self>>::decode(&mut &extra.7.encode()[..]).ok()?.into()),
		)
	}
}

/// Rialto signing params.
pub type SigningParams = sp_core::sr25519::Pair;

/// Rialto header type used in headers sync.
pub type SyncHeader = relay_substrate_client::SyncHeader<rialto_runtime::Header>;

#[cfg(test)]
mod tests {
	use super::*;
	use relay_substrate_client::TransactionEra;

	#[test]
	fn parse_transaction_works() {
		let unsigned = UnsignedTransaction {
			call: rialto_runtime::RuntimeCall::System(rialto_runtime::SystemCall::remark {
				remark: b"Hello world!".to_vec(),
			})
			.into(),
			nonce: 777,
			tip: 888,
			era: TransactionEra::immortal(),
		};
		let signed_transaction = Rialto::sign_transaction(
			SignParam {
				spec_version: 42,
				transaction_version: 50000,
				genesis_hash: [42u8; 32].into(),
				signer: sp_core::sr25519::Pair::from_seed_slice(&[1u8; 32]).unwrap(),
			},
			unsigned.clone(),
		)
		.unwrap();
		let parsed_transaction = Rialto::parse_transaction(signed_transaction).unwrap();
		assert_eq!(parsed_transaction, unsigned);
	}
}
