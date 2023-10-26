//! Unit tests for the non-fungible-token module.

#![cfg(test)]

use super::*;
use frame_support::{assert_noop, assert_ok};
use mock::*;
use orml_traits::parameters::RuntimeParameterStore;

#[test]
fn set_parameters() {
	ExtBuilder::new().execute_with(|| {
		assert_eq!(
			<ModuleParameters as RuntimeParameterStore>::get::<pallet1::Parameters, _>(pallet1::Key1),
			None
		);

		assert_noop!(
			ModuleParameters::set_parameter(
				RuntimeOrigin::signed(1),
				RuntimeParameters::Pallet1(pallet1::Parameters::Key1(pallet1::Key1, Some(123))),
			),
			DispatchError::BadOrigin
		);

		assert_ok!(ModuleParameters::set_parameter(
			RuntimeOrigin::root(),
			RuntimeParameters::Pallet1(pallet1::Parameters::Key1(pallet1::Key1, Some(123))),
		));

		assert_eq!(
			<ModuleParameters as RuntimeParameterStore>::get::<pallet1::Parameters, _>(pallet1::Key1),
			Some(123)
		);

		assert_ok!(ModuleParameters::set_parameter(
			RuntimeOrigin::root(),
			RuntimeParameters::Pallet1(pallet1::Parameters::Key2(pallet1::Key2(234), Some(345))),
		));

		assert_eq!(
			<ModuleParameters as RuntimeParameterStore>::get::<pallet1::Parameters, _>(pallet1::Key2(234)),
			Some(345)
		);

		assert_eq!(
			<ModuleParameters as RuntimeParameterStore>::get::<pallet1::Parameters, _>(pallet1::Key2(235)),
			None
		);

		assert_eq!(
			<ModuleParameters as RuntimeParameterStore>::get::<pallet2::Parameters, _>(pallet2::Key3((1, 2))),
			None
		);

		assert_noop!(
			ModuleParameters::set_parameter(
				RuntimeOrigin::root(),
				RuntimeParameters::Pallet2(pallet2::Parameters::Key3(pallet2::Key3((1, 2)), Some(123))),
			),
			DispatchError::BadOrigin
		);

		assert_ok!(ModuleParameters::set_parameter(
			RuntimeOrigin::signed(1),
			RuntimeParameters::Pallet2(pallet2::Parameters::Key3(pallet2::Key3((1, 2)), Some(456))),
		));

		assert_eq!(
			<ModuleParameters as RuntimeParameterStore>::get::<pallet2::Parameters, _>(pallet2::Key3((1, 2))),
			Some(456)
		);
	});
}
