// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! How a JAM parachain runtime hands upward messages to the Parachain Service.

use crate::Config;

impl<T: Config> crate::Pallet<T> {
	/// Send one SCALE-encoded `parachain_service_core::UpwardMessage` to the Parachain Service.
	///
	/// The caller encodes the message itself: the ABI enum lives in the service's interface crate,
	/// and encoding it here would drag every variant into this pallet's type surface for no gain.
	/// The service enforces its own per-refine budget (§4.3).
	pub fn send_jam_upward_message(message: &[u8]) {
		cumulus_jam_state_reader::jam_upward::send_upward_message(message)
	}
}
