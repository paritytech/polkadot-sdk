#[cfg(test)]
pub mod ah;
#[cfg(test)]
pub mod rc;
#[cfg(test)]
pub mod shared;

#[cfg(test)]
mod tests {
	use super::*;
	use frame::deps::frame_system;
	use xcm::v5::*;
	use codec::Decode;
	// shared tests.

	#[test]
	fn rc_session_change_reported_to_ah() {
		let mut rc = rc::ExtBuilder::default().build();
		let mut ah = ah::ExtBuilder::default().build();

		// ah-client reports session change to ah
		rc.execute_with(|| {
			// when
			assert!(frame_system::Pallet::<rc::Runtime>::block_number() == 0);
			assert!(rc::XcmQueue::get().is_empty());

			// given
			rc::roll_until_matches(|| pallet_session::CurrentIndex::<rc::Runtime>::get() == 1);

			// then
			assert_eq!(frame_system::Pallet::<rc::Runtime>::block_number(), 10);
			// end session 0, start session 1.
			assert_eq!(rc::XcmQueue::get().len(), 2);
		});

		// enacted the queued XCM message on ah.
		ah.execute_with(|| {
			rc::XcmQueue::get().into_iter().for_each(|instructions| {
				instructions.into_iter().for_each(|instruction| {
					if let Instruction::Transact { origin_kind, fallback_max_weight, call } =
						instruction
					{
						let () = call;
						// This should be decode-able as a AH call.
						let call = ah::RuntimeCall::decode_all(&mut &call[..]).unwrap();
					} else {
						// nada.
					}
				})
			})
		})

		// rc-client reports session change to staking
		// staking progresses era accordingly
		// staking calls `ElectionProvider::start` accordingly.
	}

	#[test]
	fn election_result_on_ah_reported_to_rc() {
		// when election result is complete
		// staking stores all exposures
		// validators reported to rc
		// validators enacted for next session
	}

	#[test]
	fn rc_continues_with_same_validators_if_ah_is_late() {
		// A test where ah is late to give us election result.
	}

	#[test]
	fn authoring_points_reported_to_ah_per_session() {}

	#[test]
	fn rc_is_late_to_report_session_change() {}
}
