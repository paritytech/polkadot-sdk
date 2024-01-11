// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Polkadot Bulletin Chain primitives.

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

use bp_header_chain::ChainWithGrandpa;
use bp_messages::{ChainWithMessages, MessageNonce};
use bp_runtime::{
	decl_bridge_finality_runtime_apis, decl_bridge_messages_runtime_apis,
	extensions::{
		CheckEra, CheckGenesis, CheckNonZeroSender, CheckNonce, CheckSpecVersion, CheckTxVersion,
		CheckWeight, GenericSignedExtension, GenericSignedExtensionSchema,
	},
	Chain, ChainId, TransactionEra,
};
use codec::{Decode, Encode};
use frame_support::{
	dispatch::DispatchClass,
	parameter_types,
	weights::{constants::WEIGHT_REF_TIME_PER_SECOND, Weight},
};
use frame_system::limits;
use scale_info::TypeInfo;
use sp_runtime::{traits::DispatchInfoOf, transaction_validity::TransactionValidityError, Perbill};

// This chain reuses most of Polkadot primitives.
pub use bp_polkadot_core::{
	AccountAddress, AccountId, Balance, Block, BlockNumber, Hash, Hasher, Header, Nonce, Signature,
	SignedBlock, UncheckedExtrinsic, AVERAGE_HEADER_SIZE, EXTRA_STORAGE_PROOF_SIZE,
	MAX_MANDATORY_HEADER_SIZE, REASONABLE_HEADERS_IN_JUSTIFICATON_ANCESTRY,
};

/// Maximal number of GRANDPA authorities at Polkadot Bulletin chain.
pub const MAX_AUTHORITIES_COUNT: u32 = 100;

/// Name of the With-Polkadot Bulletin chain GRANDPA pallet instance that is deployed at bridged
/// chains.
pub const WITH_POLKADOT_BULLETIN_GRANDPA_PALLET_NAME: &str = "BridgePolkadotBulletinGrandpa";
/// Name of the With-Polkadot Bulletin chain messages pallet instance that is deployed at bridged
/// chains.
pub const WITH_POLKADOT_BULLETIN_MESSAGES_PALLET_NAME: &str = "BridgePolkadotBulletinMessages";

// There are fewer system operations on this chain (e.g. staking, governance, etc.). Use a higher
// percentage of the block for data storage.
const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(90);

// Re following constants - we are using the same values at Cumulus parachains. They are limited
// by the maximal transaction weight/size. Since block limits at Bulletin Chain are larger than
// at the Cumulus Bridgeg Hubs, we could reuse the same values.

/// Maximal number of unrewarded relayer entries at inbound lane for Cumulus-based parachains.
pub const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce = 1024;

/// Maximal number of unconfirmed messages at inbound lane for Cumulus-based parachains.
pub const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce = 4096;

/// This signed extension is used to ensure that the chain transactions are signed by proper
pub type ValidateSigned = GenericSignedExtensionSchema<(), ()>;

/// Signed extension schema, used by Polkadot Bulletin.
pub type SignedExtensionSchema = GenericSignedExtension<(
	(
		CheckNonZeroSender,
		CheckSpecVersion,
		CheckTxVersion,
		CheckGenesis<Hash>,
		CheckEra<Hash>,
		CheckNonce<Nonce>,
		CheckWeight,
	),
	ValidateSigned,
)>;

/// Signed extension, used by Polkadot Bulletin.
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
pub struct SignedExtension(SignedExtensionSchema);

impl sp_runtime::traits::SignedExtension for SignedExtension {
	const IDENTIFIER: &'static str = "Not needed.";
	type AccountId = ();
	type Call = ();
	type AdditionalSigned =
		<SignedExtensionSchema as sp_runtime::traits::SignedExtension>::AdditionalSigned;
	type Pre = ();

	fn additional_signed(&self) -> Result<Self::AdditionalSigned, TransactionValidityError> {
		self.0.additional_signed()
	}

	fn pre_dispatch(
		self,
		_who: &Self::AccountId,
		_call: &Self::Call,
		_info: &DispatchInfoOf<Self::Call>,
		_len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		Ok(())
	}
}

impl SignedExtension {
	/// Create signed extension from its components.
	pub fn from_params(
		spec_version: u32,
		transaction_version: u32,
		era: TransactionEra<BlockNumber, Hash>,
		genesis_hash: Hash,
		nonce: Nonce,
	) -> Self {
		Self(GenericSignedExtension::new(
			(
				(
					(),              // non-zero sender
					(),              // spec version
					(),              // tx version
					(),              // genesis
					era.frame_era(), // era
					nonce.into(),    // nonce (compact encoding)
					(),              // Check weight
				),
				(),
			),
			Some((
				(
					(),
					spec_version,
					transaction_version,
					genesis_hash,
					era.signed_payload(genesis_hash),
					(),
					(),
				),
				(),
			)),
		))
	}

	/// Return transaction nonce.
	pub fn nonce(&self) -> Nonce {
		let common_payload = self.0.payload.0;
		common_payload.5 .0
	}
}

parameter_types! {
	/// We allow for 2 seconds of compute with a 6 second average block time.
	pub BlockWeights: limits::BlockWeights = limits::BlockWeights::with_sensible_defaults(
			Weight::from_parts(2u64 * WEIGHT_REF_TIME_PER_SECOND, u64::MAX),
			NORMAL_DISPATCH_RATIO,
		);
	// Note: Max transaction size is 8 MB. Set max block size to 10 MB to facilitate data storage.
	// This is double the "normal" Relay Chain block length limit.
	/// Maximal block length at Polkadot Bulletin chain.
	pub BlockLength: limits::BlockLength = limits::BlockLength::max_with_normal_ratio(
		10 * 1024 * 1024,
		NORMAL_DISPATCH_RATIO,
	);
}

/// Polkadot Bulletin Chain declaration.
pub struct PolkadotBulletin;

impl Chain for PolkadotBulletin {
	const ID: ChainId = *b"pdbc";

	type BlockNumber = BlockNumber;
	type Hash = Hash;
	type Hasher = Hasher;
	type Header = Header;

	type AccountId = AccountId;
	// The Bulletin Chain is a permissioned blockchain without any balances. Our `Chain` trait
	// requires balance type, which is then used by various bridge infrastructure code. However
	// this code is optional and we are not planning to use it in our bridge.
	type Balance = Balance;
	type Nonce = Nonce;
	type Signature = Signature;

	fn max_extrinsic_size() -> u32 {
		*BlockLength::get().max.get(DispatchClass::Normal)
	}

	fn max_extrinsic_weight() -> Weight {
		BlockWeights::get()
			.get(DispatchClass::Normal)
			.max_extrinsic
			.unwrap_or(Weight::MAX)
	}
}

impl ChainWithGrandpa for PolkadotBulletin {
	const WITH_CHAIN_GRANDPA_PALLET_NAME: &'static str = WITH_POLKADOT_BULLETIN_GRANDPA_PALLET_NAME;
	const MAX_AUTHORITIES_COUNT: u32 = MAX_AUTHORITIES_COUNT;
	const REASONABLE_HEADERS_IN_JUSTIFICATON_ANCESTRY: u32 =
		REASONABLE_HEADERS_IN_JUSTIFICATON_ANCESTRY;
	const MAX_MANDATORY_HEADER_SIZE: u32 = MAX_MANDATORY_HEADER_SIZE;
	const AVERAGE_HEADER_SIZE: u32 = AVERAGE_HEADER_SIZE;
}

impl ChainWithMessages for PolkadotBulletin {
	const WITH_CHAIN_MESSAGES_PALLET_NAME: &'static str =
		WITH_POLKADOT_BULLETIN_MESSAGES_PALLET_NAME;

	const MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX: MessageNonce =
		MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX;
	const MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX: MessageNonce =
		MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX;
}

decl_bridge_finality_runtime_apis!(polkadot_bulletin, grandpa);
decl_bridge_messages_runtime_apis!(polkadot_bulletin);
