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

//! Chain-specific relayer configuration.

mod kusama;
mod millau;
mod polkadot;
mod rialto;
mod rialto_parachain;
mod rococo;
mod westend;
mod wococo;

#[cfg(test)]
mod tests {
	use crate::cli::encode_message;
	use bp_messages::source_chain::TargetHeaderChain;
	use bp_runtime::Chain as _;
	use codec::Encode;
	use relay_millau_client::Millau;
	use relay_rialto_client::Rialto;
	use relay_substrate_client::{ChainWithTransactions, SignParam, UnsignedTransaction};

	#[test]
	fn maximal_rialto_to_millau_message_size_is_computed_correctly() {
		use rialto_runtime::millau_messages::MillauAsTargetHeaderChain;

		let maximal_message_size = encode_message::compute_maximal_message_size(
			bp_rialto::Rialto::max_extrinsic_size(),
			bp_millau::Millau::max_extrinsic_size(),
		);

		let message = vec![42; maximal_message_size as _];
		assert_eq!(MillauAsTargetHeaderChain::verify_message(&message), Ok(()));

		let message = vec![42; (maximal_message_size + 1) as _];
		assert!(MillauAsTargetHeaderChain::verify_message(&message).is_err());
	}

	#[test]
	fn maximal_size_remark_to_rialto_is_generated_correctly() {
		assert!(
			bridge_runtime_common::messages::target::maximal_incoming_message_size(
				bp_rialto::Rialto::max_extrinsic_size()
			) > bp_millau::Millau::max_extrinsic_size(),
			"We can't actually send maximal messages to Rialto from Millau, because Millau extrinsics can't be that large",
		)
	}
	#[test]
	fn rialto_tx_extra_bytes_constant_is_correct() {
		let rialto_call = rialto_runtime::RuntimeCall::System(rialto_runtime::SystemCall::remark {
			remark: vec![],
		});
		let rialto_tx = Rialto::sign_transaction(
			SignParam {
				spec_version: 1,
				transaction_version: 1,
				genesis_hash: Default::default(),
				signer: sp_keyring::AccountKeyring::Alice.pair(),
			},
			UnsignedTransaction::new(rialto_call.clone().into(), 0),
		)
		.unwrap();
		let extra_bytes_in_transaction = rialto_tx.encode().len() - rialto_call.encode().len();
		assert!(
			bp_rialto::TX_EXTRA_BYTES as usize >= extra_bytes_in_transaction,
			"Hardcoded number of extra bytes in Rialto transaction {} is lower than actual value: {}",
			bp_rialto::TX_EXTRA_BYTES,
			extra_bytes_in_transaction,
		);
	}

	#[test]
	fn millau_tx_extra_bytes_constant_is_correct() {
		let millau_call = millau_runtime::RuntimeCall::System(millau_runtime::SystemCall::remark {
			remark: vec![],
		});
		let millau_tx = Millau::sign_transaction(
			SignParam {
				spec_version: 0,
				transaction_version: 0,
				genesis_hash: Default::default(),
				signer: sp_keyring::AccountKeyring::Alice.pair(),
			},
			UnsignedTransaction::new(millau_call.clone().into(), 0),
		)
		.unwrap();
		let extra_bytes_in_transaction = millau_tx.encode().len() - millau_call.encode().len();
		assert!(
			bp_millau::TX_EXTRA_BYTES as usize >= extra_bytes_in_transaction,
			"Hardcoded number of extra bytes in Millau transaction {} is lower than actual value: {}",
			bp_millau::TX_EXTRA_BYTES,
			extra_bytes_in_transaction,
		);
	}
}
