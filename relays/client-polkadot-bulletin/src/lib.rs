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

//! Types used to connect to the Polkadot Bulletin chain.

mod codegen_runtime;

use bp_polkadot_bulletin::POLKADOT_BULLETIN_SYNCED_HEADERS_GRANDPA_INFO_METHOD;
use codec::Encode;
use relay_substrate_client::{
	Chain, ChainWithBalances, ChainWithGrandpa, ChainWithMessages, ChainWithRuntimeVersion,
	ChainWithTransactions, Error as SubstrateError, SignParam, SimpleRuntimeVersion,
	UnderlyingChainProvider, UnsignedTransaction,
};
use sp_core::{storage::StorageKey, Pair};
use sp_runtime::{generic::SignedPayload, traits::IdentifyAccount, MultiAddress};
use sp_session::MembershipProof;
use std::time::Duration;

pub use codegen_runtime::api::runtime_types;

/// Call of the Polkadot Bulletin Chain runtime.
pub type RuntimeCall = runtime_types::polkadot_bulletin_chain_runtime::RuntimeCall;
/// Call of the `Sudo` pallet.
pub type SudoCall = runtime_types::pallet_sudo::pallet::Call;
/// Call of the GRANDPA pallet.
pub type GrandpaCall = runtime_types::pallet_grandpa::pallet::Call;
/// Call of the with-PolkadotBridgeHub bridge GRANDPA pallet.
pub type BridgePolkadotGrandpaCall = runtime_types::pallet_bridge_grandpa::pallet::Call;
/// Call of the with-PolkadotBridgeHub bridge parachains pallet.
pub type BridgePolkadotParachainsCall = runtime_types::pallet_bridge_parachains::pallet::Call;
/// Call of the with-PolkadotBridgeHub bridge messages pallet.
pub type BridgePolkadotMessagesCall = runtime_types::pallet_bridge_messages::pallet::Call;

/// Polkadot header id.
pub type HeaderId =
	relay_utils::HeaderId<bp_polkadot_bulletin::Hash, bp_polkadot_bulletin::BlockNumber>;

/// Polkadot header type used in headers sync.
pub type SyncHeader = relay_substrate_client::SyncHeader<bp_polkadot_bulletin::Header>;

/// The address format for describing accounts.
pub type Address = MultiAddress<bp_polkadot_bulletin::AccountId, ()>;

/// Polkadot chain definition
#[derive(Debug, Clone, Copy)]
pub struct PolkadotBulletin;

impl UnderlyingChainProvider for PolkadotBulletin {
	type Chain = bp_polkadot_bulletin::PolkadotBulletin;
}

impl Chain for PolkadotBulletin {
	const NAME: &'static str = "PolkadotBulletin";
	const BEST_FINALIZED_HEADER_ID_METHOD: &'static str =
		bp_polkadot_bulletin::BEST_FINALIZED_POLKADOT_BULLETIN_HEADER_METHOD;
	const AVERAGE_BLOCK_INTERVAL: Duration = Duration::from_secs(6);

	type SignedBlock = bp_polkadot_bulletin::SignedBlock;
	type Call = RuntimeCall;
}

impl ChainWithGrandpa for PolkadotBulletin {
	const SYNCED_HEADERS_GRANDPA_INFO_METHOD: &'static str =
		POLKADOT_BULLETIN_SYNCED_HEADERS_GRANDPA_INFO_METHOD;

	type KeyOwnerProof = MembershipProof;
}

impl ChainWithMessages for PolkadotBulletin {
	// this is not critical (some metrics will be missing from the storage), but probably it needs
	// to be changed when we'll polish the bridge configuration
	const WITH_CHAIN_RELAYERS_PALLET_NAME: Option<&'static str> = None;

	const TO_CHAIN_MESSAGE_DETAILS_METHOD: &'static str =
		bp_polkadot_bulletin::TO_POLKADOT_BULLETIN_MESSAGE_DETAILS_METHOD;
	const FROM_CHAIN_MESSAGE_DETAILS_METHOD: &'static str =
		bp_polkadot_bulletin::FROM_POLKADOT_BULLETIN_MESSAGE_DETAILS_METHOD;
}

impl ChainWithBalances for PolkadotBulletin {
	fn account_info_storage_key(_account_id: &Self::AccountId) -> StorageKey {
		// no balances at this chain
		StorageKey(vec![])
	}
}

impl ChainWithTransactions for PolkadotBulletin {
	type AccountKeyPair = sp_core::sr25519::Pair;
	type SignedTransaction = bp_polkadot_bulletin::UncheckedExtrinsic<
		Self::Call,
		bp_polkadot_bulletin::TransactionExtension,
	>;

	fn sign_transaction(
		param: SignParam<Self>,
		unsigned: UnsignedTransaction<Self>,
	) -> Result<Self::SignedTransaction, SubstrateError> {
		let raw_payload = SignedPayload::new(
			unsigned.call,
			bp_polkadot_bulletin::TransactionExtension::from_params(
				param.spec_version,
				param.transaction_version,
				unsigned.era,
				param.genesis_hash,
				unsigned.nonce,
			),
		)?;

		let signature = raw_payload.using_encoded(|payload| param.signer.sign(payload));
		let signer: sp_runtime::MultiSigner = param.signer.public().into();
		let (call, extra, _) = raw_payload.deconstruct();

		Ok(Self::SignedTransaction::new_signed(
			call,
			signer.into_account().into(),
			signature.into(),
			extra,
		))
	}
}

impl ChainWithRuntimeVersion for PolkadotBulletin {
	const RUNTIME_VERSION: Option<SimpleRuntimeVersion> =
		Some(SimpleRuntimeVersion { spec_version: 100, transaction_version: 1 });
}
