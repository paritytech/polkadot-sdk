// Copyright 2022 Parity Technologies (UK) Ltd.
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

//! Types used to connect to the BridgeHub-Rococo-Substrate parachain.

pub mod codegen_runtime;

use bp_bridge_hub_rococo::{BridgeHubSignedExtension, SignedExtension, AVERAGE_BLOCK_INTERVAL};
use bp_messages::MessageNonce;
use bp_runtime::ChainId;
use codec::Encode;
use relay_substrate_client::{
	calls::UtilityCall as MockUtilityCall, Chain, ChainWithBalances, ChainWithMessages,
	ChainWithTransactions, ChainWithUtilityPallet, Error as SubstrateError,
	MockedRuntimeUtilityPallet, SignParam, UnderlyingChainProvider, UnsignedTransaction,
};
use sp_core::{storage::StorageKey, Pair};
use sp_runtime::{generic::SignedPayload, traits::IdentifyAccount};
use std::time::Duration;

pub use codegen_runtime::api::runtime_types;

pub type RuntimeCall = runtime_types::bridge_hub_rococo_runtime::RuntimeCall;
pub type BridgeMessagesCall = runtime_types::pallet_bridge_messages::pallet::Call;
pub type BridgeGrandpaCall = runtime_types::pallet_bridge_grandpa::pallet::Call;
pub type BridgeParachainCall = runtime_types::pallet_bridge_parachains::pallet::Call;
type UncheckedExtrinsic = bp_bridge_hub_rococo::UncheckedExtrinsic<RuntimeCall, SignedExtension>;
type UtilityCall = runtime_types::pallet_utility::pallet::Call;

/// Rococo chain definition
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BridgeHubRococo;

impl UnderlyingChainProvider for BridgeHubRococo {
	type Chain = bp_bridge_hub_rococo::BridgeHubRococo;
}

impl Chain for BridgeHubRococo {
	const ID: ChainId = bp_runtime::BRIDGE_HUB_ROCOCO_CHAIN_ID;
	const NAME: &'static str = "BridgeHubRococo";
	const BEST_FINALIZED_HEADER_ID_METHOD: &'static str =
		bp_bridge_hub_rococo::BEST_FINALIZED_BRIDGE_HUB_ROCOCO_HEADER_METHOD;
	const AVERAGE_BLOCK_INTERVAL: Duration = AVERAGE_BLOCK_INTERVAL;

	type SignedBlock = bp_bridge_hub_rococo::SignedBlock;
	type Call = RuntimeCall;
}

impl ChainWithBalances for BridgeHubRococo {
	fn account_info_storage_key(account_id: &Self::AccountId) -> StorageKey {
		bp_bridge_hub_rococo::AccountInfoStorageMapKeyProvider::final_key(account_id)
	}
}

impl From<MockUtilityCall<RuntimeCall>> for RuntimeCall {
	fn from(value: MockUtilityCall<RuntimeCall>) -> RuntimeCall {
		match value {
			MockUtilityCall::batch_all(calls) =>
				RuntimeCall::Utility(UtilityCall::batch_all { calls }),
		}
	}
}

impl ChainWithUtilityPallet for BridgeHubRococo {
	type UtilityPallet = MockedRuntimeUtilityPallet<RuntimeCall>;
}

impl ChainWithTransactions for BridgeHubRococo {
	type AccountKeyPair = sp_core::sr25519::Pair;
	type SignedTransaction = UncheckedExtrinsic;

	fn sign_transaction(
		param: SignParam<Self>,
		unsigned: UnsignedTransaction<Self>,
	) -> Result<Self::SignedTransaction, SubstrateError> {
		let raw_payload = SignedPayload::new(
			unsigned.call,
			SignedExtension::from_params(
				param.spec_version,
				param.transaction_version,
				unsigned.era,
				param.genesis_hash,
				unsigned.nonce,
				unsigned.tip,
			),
		)?;

		let signature = raw_payload.using_encoded(|payload| param.signer.sign(payload));
		let signer: sp_runtime::MultiSigner = param.signer.public().into();
		let (call, extra, _) = raw_payload.deconstruct();

		Ok(UncheckedExtrinsic::new_signed(
			call,
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
				*address == bp_bridge_hub_rococo::Address::Id(signer.public().into())
			})
			.unwrap_or(false)
	}

	fn parse_transaction(tx: Self::SignedTransaction) -> Option<UnsignedTransaction<Self>> {
		let extra = &tx.signature.as_ref()?.2;
		Some(UnsignedTransaction::new(tx.function, extra.nonce()).tip(extra.tip()))
	}
}

impl ChainWithMessages for BridgeHubRococo {
	const WITH_CHAIN_MESSAGES_PALLET_NAME: &'static str =
		bp_bridge_hub_rococo::WITH_BRIDGE_HUB_ROCOCO_MESSAGES_PALLET_NAME;
	const WITH_CHAIN_RELAYERS_PALLET_NAME: Option<&'static str> =
		Some(bp_bridge_hub_rococo::WITH_BRIDGE_HUB_ROCOCO_RELAYERS_PALLET_NAME);

	const TO_CHAIN_MESSAGE_DETAILS_METHOD: &'static str =
		bp_bridge_hub_rococo::TO_BRIDGE_HUB_ROCOCO_MESSAGE_DETAILS_METHOD;
	const FROM_CHAIN_MESSAGE_DETAILS_METHOD: &'static str =
		bp_bridge_hub_rococo::FROM_BRIDGE_HUB_ROCOCO_MESSAGE_DETAILS_METHOD;

	const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce =
		bp_bridge_hub_rococo::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX;
	const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce =
		bp_bridge_hub_rococo::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX;
}

#[cfg(test)]
mod tests {
	use super::*;
	use relay_substrate_client::TransactionEra;

	type SystemCall = runtime_types::frame_system::pallet::Call;

	#[test]
	fn parse_transaction_works() {
		let unsigned = UnsignedTransaction {
			call: RuntimeCall::System(SystemCall::remark { remark: b"Hello world!".to_vec() })
				.into(),
			nonce: 777,
			tip: 888,
			era: TransactionEra::immortal(),
		};
		let signed_transaction = BridgeHubRococo::sign_transaction(
			SignParam {
				spec_version: 42,
				transaction_version: 50000,
				genesis_hash: [42u8; 32].into(),
				signer: sp_core::sr25519::Pair::from_seed_slice(&[1u8; 32]).unwrap(),
			},
			unsigned.clone(),
		)
		.unwrap();
		let parsed_transaction = BridgeHubRococo::parse_transaction(signed_transaction).unwrap();
		assert_eq!(parsed_transaction, unsigned);
	}
}
