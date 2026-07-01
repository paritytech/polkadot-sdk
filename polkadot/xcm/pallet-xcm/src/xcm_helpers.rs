// Copyright (C) Parity Technologies (UK) Ltd.
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

use xcm::latest::XcmHash;

/// Finds the message ID of the first `XcmPallet::Sent` event in the given events.
pub fn find_xcm_sent_message_id<T>(
	events: impl IntoIterator<Item = <T as crate::Config>::RuntimeEvent>,
) -> Option<XcmHash>
where
	T: crate::Config,
	<T as crate::Config>::RuntimeEvent: TryInto<crate::Event<T>>,
{
	events.into_iter().find_map(|event| {
		if let Ok(crate::Event::Sent { message_id, .. }) = event.try_into() {
			Some(message_id)
		} else {
			None
		}
	})
}
