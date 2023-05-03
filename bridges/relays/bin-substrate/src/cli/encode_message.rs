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

use crate::cli::{ExplicitOrMaximal, HexBytes};
use bp_runtime::EncodedOrDecodedCall;
use bridge_runtime_common::CustomNetworkId;
use codec::Encode;
use frame_support::weights::Weight;
use relay_substrate_client::Chain;
use structopt::StructOpt;
use xcm::latest::prelude::*;

/// All possible messages that may be delivered to generic Substrate chain.
///
/// Note this enum may be used in the context of both Source (as part of `encode-call`)
/// and Target chain (as part of `encode-message/send-message`).
#[derive(StructOpt, Debug, PartialEq, Eq)]
pub enum Message {
	/// Raw bytes for the message.
	Raw {
		/// Raw message bytes.
		data: HexBytes,
	},
	/// Message with given size.
	Sized {
		/// Sized of the message.
		size: ExplicitOrMaximal<u32>,
	},
}

/// Raw, SCALE-encoded message payload used in expected deployment.
pub type RawMessage = Vec<u8>;

pub trait CliEncodeMessage: Chain {
	/// Returns dummy `AccountId32` universal source given this network id.
	fn dummy_universal_source() -> anyhow::Result<xcm::v3::Junctions> {
		use xcm::v3::prelude::*;

		let this_network = CustomNetworkId::try_from(Self::ID)
			.map(|n| n.as_network_id())
			.map_err(|_| anyhow::format_err!("Unsupported chain: {:?}", Self::ID))?;
		Ok(X2(
			GlobalConsensus(this_network),
			AccountId32 { network: Some(this_network), id: [0u8; 32] },
		))
	}

	/// Returns XCM blob that is passed to the `send_message` function of the messages pallet
	/// and then is sent over the wire.
	fn encode_wire_message(target: NetworkId, at_target_xcm: Xcm<()>) -> anyhow::Result<Vec<u8>>;
	/// Encode an `execute` XCM call of the XCM pallet.
	fn encode_execute_xcm(
		message: xcm::VersionedXcm<Self::Call>,
	) -> anyhow::Result<EncodedOrDecodedCall<Self::Call>>;

	/// Estimate value of `max_weight` argument for the `execute` XCM call of the XCM pallet.
	fn estimate_execute_xcm_weight() -> Weight {
		// we are only executing XCM on our testnets and 1/100 of max extrinsic weight is ok
		Self::max_extrinsic_weight() / 100
	}
}

/// Encode message payload passed through CLI flags.
pub(crate) fn encode_message<Source: CliEncodeMessage, Target: Chain>(
	message: &Message,
) -> anyhow::Result<RawMessage> {
	Ok(match message {
		Message::Raw { ref data } => data.0.clone(),
		Message::Sized { ref size } => {
			let destination = CustomNetworkId::try_from(Target::ID)
				.map(|n| n.as_network_id())
				.map_err(|_| anyhow::format_err!("Unsupported target chain: {:?}", Target::ID))?;
			let expected_size = match *size {
				ExplicitOrMaximal::Explicit(size) => size,
				ExplicitOrMaximal::Maximal => compute_maximal_message_size(
					Source::max_extrinsic_size(),
					Target::max_extrinsic_size(),
				),
			} as usize;

			let at_target_xcm = vec![ExpectPallet {
				index: 0,
				name: vec![42; expected_size],
				module_name: vec![],
				crate_major: 0,
				min_crate_minor: 0,
			}]
			.into();
			let at_target_xcm_size =
				Source::encode_wire_message(destination, at_target_xcm)?.encoded_size();
			let at_target_xcm_overhead = at_target_xcm_size.saturating_sub(expected_size);
			let at_target_xcm = vec![ExpectPallet {
				index: 0,
				name: vec![42; expected_size.saturating_sub(at_target_xcm_overhead)],
				module_name: vec![],
				crate_major: 0,
				min_crate_minor: 0,
			}]
			.into();

			xcm::VersionedXcm::<()>::V3(
				vec![ExportMessage {
					network: destination,
					destination: destination.into(),
					xcm: at_target_xcm,
				}]
				.into(),
			)
			.encode()
		},
	})
}

/// Compute maximal message size, given max extrinsic size at source and target chains.
pub(crate) fn compute_maximal_message_size(
	maximal_source_extrinsic_size: u32,
	maximal_target_extrinsic_size: u32,
) -> u32 {
	// assume that both signed extensions and other arguments fit 1KB
	let service_tx_bytes_on_source_chain = 1024;
	let maximal_source_extrinsic_size =
		maximal_source_extrinsic_size - service_tx_bytes_on_source_chain;
	let maximal_message_size =
		bridge_runtime_common::messages::target::maximal_incoming_message_size(
			maximal_target_extrinsic_size,
		);
	std::cmp::min(maximal_message_size, maximal_source_extrinsic_size)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::send_message::decode_xcm;
	use bp_runtime::Chain;
	use relay_millau_client::Millau;
	use relay_rialto_client::Rialto;

	fn approximate_message_size<Source: CliEncodeMessage>(xcm_msg_len: usize) -> usize {
		xcm_msg_len + Source::dummy_universal_source().unwrap().encoded_size()
	}

	#[test]
	fn encode_explicit_size_message_works() {
		let msg = encode_message::<Rialto, Millau>(&Message::Sized {
			size: ExplicitOrMaximal::Explicit(100),
		})
		.unwrap();
		// since it isn't the returned XCM what is sent over the wire, we can only check if
		// it is close to what we need
		assert!(
			(1f64 - (approximate_message_size::<Rialto>(msg.len()) as f64) / 100_f64).abs() < 0.1
		);
		// check that it decodes to valid xcm
		let _ = decode_xcm::<()>(msg).unwrap();
	}

	#[test]
	fn encode_maximal_size_message_works() {
		let maximal_size = compute_maximal_message_size(
			Rialto::max_extrinsic_size(),
			Millau::max_extrinsic_size(),
		);

		let msg =
			encode_message::<Rialto, Millau>(&Message::Sized { size: ExplicitOrMaximal::Maximal })
				.unwrap();
		// since it isn't the returned XCM what is sent over the wire, we can only check if
		// it is close to what we need
		assert!(
			(1f64 - approximate_message_size::<Rialto>(msg.len()) as f64 / maximal_size as f64)
				.abs() < 0.1
		);
		// check that it decodes to valid xcm
		let _ = decode_xcm::<()>(msg).unwrap();
	}
}
