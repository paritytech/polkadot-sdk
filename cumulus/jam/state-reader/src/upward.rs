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

use sp_runtime_interface::{pass_by::PassFatPointerAndRead, runtime_interface};

/// Runtime interface for sending upward messages to the Parachain Service.
#[runtime_interface]
pub trait JamUpward {
	/// Send one SCALE-encoded `parachain_service_core::UpwardMessage` to the Parachain Service.
	///
	/// Only the PVF can deliver a message: `validate_block` replaces this with the service's
	/// `send_upward_message` host call. When building or importing a block the message has no
	/// receiver, so the node throws it away.
	#[polkavm_index(345)]
	fn send_upward_message(_message: PassFatPointerAndRead<&[u8]>) {}
}
