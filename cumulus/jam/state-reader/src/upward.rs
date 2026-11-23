// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The host function a JAM parachain runtime sends its upward messages through.

#[cfg(not(all(jam, substrate_runtime)))]
use sp_runtime_interface::{pass_by::PassFatPointerAndRead, runtime_interface};

/// Runtime interface for sending upward messages to the Parachain Service.
#[cfg(not(all(jam, substrate_runtime)))]
#[runtime_interface]
pub trait JamUpward {
	/// Send one upward message to the Parachain Service: the SCALE encoding of a
	/// `(parachain_service_core::types::MessageId,
	/// parachain_service_core::upward_message::UpwardMessage)` pair. Prefer the typed
	/// [`send_upward_message`](super::send_upward_message).
	///
	/// Only the PVF can deliver a message: `validate_block` replaces this with the service's
	/// `send_upward_message` host call. When building or importing a block the message has no
	/// receiver, so the node throws it away.
	fn send_upward_message(_message: PassFatPointerAndRead<&[u8]>) {}
}

/// The JAM (PolkaVM) side of [`JamUpward`]: an import at its fixed PolkaVM index, named like the
/// `#[runtime_interface]` host function so the node serves it through that registration.
#[cfg(all(jam, substrate_runtime))]
pub mod jam_upward {
	use codec::Encode;
	use parachain_service_core::{types::MessageId, upward_message::UpwardMessage};

	sp_jam_runtime_interface::host_calls! {
		/// Send one SCALE-encoded `(MessageId, UpwardMessage)` pair; the node throws it away.
		#[index = 342]
		fn ext_jam_upward_send_upward_message_version_1(message: u64);
	}

	fn host_call(message_id: MessageId, message: &UpwardMessage) {
		let encoded = (message_id, message).encode();
		let encoded =
			sp_runtime_interface::pack_ptr_and_len(encoded.as_ptr() as u32, encoded.len() as u32);
		// SAFETY: The packed pointer and length come from a live buffer.
		unsafe { ext_jam_upward_send_upward_message_version_1(encoded) }
	}

	/// Exchangeable implementation of [`send_upward_message`](super::send_upward_message);
	/// `validate_block` replaces it with the Parachain Service's `send_upward_message` host call,
	/// which takes the same arguments.
	#[allow(non_upper_case_globals)]
	pub static host_send_upward_message: sp_runtime_interface::wasm::ExchangeableFunction<
		fn(MessageId, &UpwardMessage),
	> = sp_runtime_interface::wasm::ExchangeableFunction::new(host_call);
}

/// Send `message` to the Parachain Service, tagged with `message_id`.
///
/// The service logs every `AccumulateLog` the message causes under `message_id`, so the id is how
/// the runtime matches the service's feedback to what it sent.
#[cfg(any(jam, not(substrate_runtime)))]
pub fn send_upward_message(
	message_id: parachain_service_core::types::MessageId,
	message: &parachain_service_core::upward_message::UpwardMessage,
) {
	#[cfg(all(jam, substrate_runtime))]
	jam_upward::host_send_upward_message.get()(message_id, message);
	#[cfg(not(substrate_runtime))]
	jam_upward::send_upward_message(&codec::Encode::encode(&(message_id, message)));
}
