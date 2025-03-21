use super::*;

#[test]
fn unexpected_activation_timestamp() {}

#[test]
fn forcing_force_none() {
	ExtBuilder::default().build_and_execute(|| {
		ForceEra::<T>::put(Forcing::ForceNone);

		Session::roll_to_next_session();
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::SessionRotated { starting_session: 4, active_era: 1, planned_era: 1 }]
		);

		Session::roll_to_next_session();
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::SessionRotated { starting_session: 5, active_era: 1, planned_era: 1 }]
		);

		Session::roll_to_next_session();
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::SessionRotated { starting_session: 6, active_era: 1, planned_era: 1 }]
		);

		Session::roll_to_next_session();
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::SessionRotated { starting_session: 7, active_era: 1, planned_era: 1 }]
		);

		Session::roll_to_next_session();
		assert_eq!(
			staking_events_since_last_call(),
			vec![Event::SessionRotated { starting_session: 8, active_era: 1, planned_era: 1 }]
		);
	});
}

#[test]
fn forcing_no_forcing_default() {
	todo!()
}

#[test]
fn forcing_force_always() {
	todo!()
}

#[test]
fn forcing_force_new() {
	todo!()
}
