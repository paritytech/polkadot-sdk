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
use codec::Encode;
use frame_support::weights::Weight;
use relay_substrate_client::Chain;
use structopt::StructOpt;

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
pub(crate) fn encode_message<Source: Chain, Target: Chain>(
	message: &Message,
) -> anyhow::Result<RawMessage> {
	Ok(match message {
		Message::Raw { ref data } => data.0.clone(),
		Message::Sized { ref size } => {
			let expected_xcm_size = match *size {
				ExplicitOrMaximal::Explicit(size) => size,
				ExplicitOrMaximal::Maximal => compute_maximal_message_size(
					Source::max_extrinsic_size(),
					Target::max_extrinsic_size(),
				),
			};

			// there's no way to craft XCM of the given size - we'll be using `ExpectPallet`
			// instruction, which has byte vector inside
			let mut current_vec_size = expected_xcm_size;
			let xcm = loop {
				let xcm = xcm::VersionedXcm::<()>::V3(
					vec![xcm::v3::Instruction::ExpectPallet {
						index: 0,
						name: vec![42; current_vec_size as usize],
						module_name: vec![],
						crate_major: 0,
						min_crate_minor: 0,
					}]
					.into(),
				);
				if xcm.encode().len() <= expected_xcm_size as usize {
					break xcm
				}

				current_vec_size -= 1;
			};
			xcm.encode()
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
	if maximal_message_size > maximal_source_extrinsic_size {
		maximal_source_extrinsic_size
	} else {
		maximal_message_size
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::send_message::decode_xcm;
	use bp_runtime::Chain;
	use relay_millau_client::Millau;
	use relay_rialto_client::Rialto;

	#[test]
	fn encode_explicit_size_message_works() {
		let msg = encode_message::<Rialto, Millau>(&Message::Sized {
			size: ExplicitOrMaximal::Explicit(100),
		})
		.unwrap();
		assert_eq!(msg.len(), 100);
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
		assert_eq!(msg.len(), maximal_size as usize);
		// check that it decodes to valid xcm
		let _ = decode_xcm::<()>(msg).unwrap();
	}
}
