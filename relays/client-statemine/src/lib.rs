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

//! Types used to connect to the Statemine chain.

use codec::Encode;
use frame_support::weights::Weight;
use relay_substrate_client::{
	Chain, ChainBase, ChainWithTransactions, Error as SubstrateError, SignParam,
	UnsignedTransaction,
};
use sp_core::Pair;
use sp_runtime::{generic::SignedPayload, traits::IdentifyAccount};
use std::time::Duration;

pub mod runtime;

/// Statemine chain definition
#[derive(Debug, Clone, Copy)]
pub struct Statemine;

impl ChainBase for Statemine {
	type BlockNumber = bp_statemine::BlockNumber;
	type Hash = bp_statemine::Hash;
	type Hasher = bp_statemine::Hasher;
	type Header = bp_statemine::Header;

	type AccountId = bp_statemine::AccountId;
	type Balance = bp_statemine::Balance;
	type Index = bp_statemine::Nonce;
	type Signature = bp_statemine::Signature;

	fn max_extrinsic_size() -> u32 {
		bp_statemine::Statemine::max_extrinsic_size()
	}

	fn max_extrinsic_weight() -> Weight {
		bp_statemine::Statemine::max_extrinsic_weight()
	}
}

impl Chain for Statemine {
	const NAME: &'static str = "Statemine";
	const TOKEN_ID: Option<&'static str> = Some("kusama");
	const BEST_FINALIZED_HEADER_ID_METHOD: &'static str = "<unused>";
	const AVERAGE_BLOCK_INTERVAL: Duration = Duration::from_secs(6);
	const STORAGE_PROOF_OVERHEAD: u32 = bp_statemine::EXTRA_STORAGE_PROOF_SIZE;

	type SignedBlock = bp_statemine::SignedBlock;
	type Call = runtime::Call;
}

impl ChainWithTransactions for Statemine {
	type AccountKeyPair = sp_core::sr25519::Pair;
	type SignedTransaction = runtime::UncheckedExtrinsic;

	fn sign_transaction(
		param: SignParam<Self>,
		unsigned: UnsignedTransaction<Self>,
	) -> Result<Self::SignedTransaction, SubstrateError> {
		let raw_payload = SignedPayload::new(
			unsigned.call.clone(),
			bp_statemine::SignedExtensions::new(
				param.spec_version,
				param.transaction_version,
				unsigned.era,
				param.genesis_hash,
				unsigned.nonce,
				unsigned.tip,
			),
		)
		.expect("SignedExtension never fails.");
		let signature = raw_payload.using_encoded(|payload| param.signer.sign(payload));
		let signer: sp_runtime::MultiSigner = param.signer.public().into();
		let (call, extra, _) = raw_payload.deconstruct();

		Ok(runtime::UncheckedExtrinsic::new_signed(
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
			.map(|(address, _, _)| *address == bp_statemine::Address::Id(signer.public().into()))
			.unwrap_or(false)
	}

	fn parse_transaction(_tx: Self::SignedTransaction) -> Option<UnsignedTransaction<Self>> {
		unimplemented!("not used on Statemine")
	}
}
