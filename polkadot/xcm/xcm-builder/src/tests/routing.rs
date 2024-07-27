// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use super::*;
use frame_support::{assert_ok, traits::Everything};
use xcm_executor::traits::Properties;

fn props() -> Properties {
	Properties { weight_credit: Weight::zero(), message_id: None }
}

#[test]
fn trailing_set_topic_as_id_with_unique_topic_should_work() {
	type AllowSubscriptions = AllowSubscriptionsFrom<Everything>;

	// check the validity of XCM for the `AllowSubscriptions` barrier
	let valid_xcm = Xcm::<()>(vec![SubscribeVersion {
		query_id: 42,
		max_response_weight: Weight::from_parts(5000, 5000),
	}]);
	assert_eq!(
		AllowSubscriptions::should_execute(
			&Location::parent(),
			valid_xcm.clone().inner_mut(),
			Weight::from_parts(10, 10),
			&mut props(),
		),
		Ok(())
	);

	// simulate sending `valid_xcm` with the `WithUniqueTopic` router
	let mut sent_xcm = sp_io::TestExternalities::default().execute_with(|| {
		assert_ok!(send_xcm::<WithUniqueTopic<TestMessageSender>>(Location::parent(), valid_xcm,));
		sent_xcm()
	});
	assert_eq!(1, sent_xcm.len());

	// `sent_xcm` should contain `SubscribeVersion` and have `SetTopic` added
	let mut sent_xcm = sent_xcm.remove(0).1;
	let _ = sent_xcm
		.0
		.matcher()
		.assert_remaining_insts(2)
		.expect("two instructions")
		.match_next_inst(|instr| match instr {
			SubscribeVersion { .. } => Ok(()),
			_ => Err(ProcessMessageError::BadFormat),
		})
		.expect("expected instruction `SubscribeVersion`")
		.match_next_inst(|instr| match instr {
			SetTopic(..) => Ok(()),
			_ => Err(ProcessMessageError::BadFormat),
		})
		.expect("expected instruction `SetTopic`");

	// `sent_xcm` contains `SetTopic` and is now invalid for `AllowSubscriptions`
	assert_eq!(
		AllowSubscriptions::should_execute(
			&Location::parent(),
			sent_xcm.clone().inner_mut(),
			Weight::from_parts(10, 10),
			&mut props(),
		),
		Err(ProcessMessageError::BadFormat)
	);

	// let's apply `TrailingSetTopicAsId` before `AllowSubscriptions`
	let mut props = props();
	assert!(props.message_id.is_none());

	// should pass, and the `message_id` is set
	assert_eq!(
		TrailingSetTopicAsId::<AllowSubscriptions>::should_execute(
			&Location::parent(),
			sent_xcm.clone().inner_mut(),
			Weight::from_parts(10, 10),
			&mut props,
		),
		Ok(())
	);
	assert!(props.message_id.is_some());
}
