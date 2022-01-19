// Copyright (C) 2021 Parity Technologies (UK) Ltd.
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

use super::*;
use cumulus_primitives_core::XcmpMessageHandler;
use frame_support::assert_noop;
use mock::{new_test_ext, Origin, Test, XcmpQueue};

#[test]
fn one_message_does_not_panic() {
	new_test_ext().execute_with(|| {
		let message_format = XcmpMessageFormat::ConcatenatedVersionedXcm.encode();
		let messages = vec![(Default::default(), 1u32.into(), message_format.as_slice())];

		// This shouldn't cause a panic
		XcmpQueue::handle_xcmp_messages(messages.into_iter(), Weight::max_value());
	})
}

#[test]
#[should_panic = "Invalid incoming blob message data"]
#[cfg(debug_assertions)]
fn bad_message_is_handled() {
	new_test_ext().execute_with(|| {
		let bad_data = vec![
			1, 1, 3, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 64, 239, 139, 0,
			0, 0, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 0, 0, 0, 0, 0, 0, 0, 37, 0,
			0, 0, 0, 0, 0, 0, 16, 0, 127, 147,
		];
		InboundXcmpMessages::<Test>::insert(ParaId::from(1000), 1, bad_data);
		let format = XcmpMessageFormat::ConcatenatedEncodedBlob;
		// This should exit with an error.
		XcmpQueue::process_xcmp_message(1000.into(), (1, format), 10_000_000_000, 10_000_000_000);
	});
}

/// Tests that a blob message is handled. Currently this isn't implemented and panics when debug assertions
/// are enabled. When this feature is enabled, this test should be rewritten properly.
#[test]
#[should_panic = "Blob messages not handled."]
#[cfg(debug_assertions)]
fn handle_blob_message() {
	new_test_ext().execute_with(|| {
		let bad_data = vec![
			1, 1, 1, 1, 3, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 64, 239,
			139, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 0, 0, 0, 0, 0, 0, 0,
			37, 0, 0, 0, 0, 0, 0, 0, 16, 0, 127, 147,
		];
		InboundXcmpMessages::<Test>::insert(ParaId::from(1000), 1, bad_data);
		let format = XcmpMessageFormat::ConcatenatedEncodedBlob;
		XcmpQueue::process_xcmp_message(1000.into(), (1, format), 10_000_000_000, 10_000_000_000);
	});
}

#[test]
fn service_overweight_unknown() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			XcmpQueue::service_overweight(Origin::root(), 0, 1000),
			Error::<Test>::BadOverweightIndex,
		);
	});
}

#[test]
fn service_overweight_bad_xcm_format() {
	new_test_ext().execute_with(|| {
		let bad_xcm = vec![255];
		Overweight::<Test>::insert(0, (ParaId::from(1000), 0, bad_xcm));

		assert_noop!(XcmpQueue::service_overweight(Origin::root(), 0, 1000), Error::<Test>::BadXcm);
	});
}
