// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! How a JAM parachain runtime hands upward messages to the Parachain Service.

use crate::Config;
use parachain_service_core::{types::MessageId, upward_message::UpwardMessage};

impl<T: Config> crate::Pallet<T> {
	/// Send `message` to the Parachain Service, tagged with `message_id`.
	///
	/// The service logs every `AccumulateLog` the message causes under `message_id` (see
	/// [`crate::feedback`]). It enforces its own per-refine budget (§4.3).
	pub fn send_upward_message(message_id: MessageId, message: &UpwardMessage) {
		cumulus_jam_state_reader::upward::send_upward_message(message_id, message)
	}
}
