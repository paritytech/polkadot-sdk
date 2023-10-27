//! Unit tests for the non-fungible-token module.

#![cfg(test)]

use super::*;
use frame_support::{assert_noop, assert_ok};
use mock::*;
use RuntimeOrigin as Origin;

#[docify::export]
#[test]
fn set_parameters_example() {
	use RuntimeParameters::*;

	ExtBuilder::new().execute_with(|| {
		assert_eq!(pallet1::Key3::get(), 2, "Default works");

		// This gets rejected since the origin is not root.
		assert_noop!(
			ModuleParameters::set_parameter(
				Origin::signed(1),
				Pallet1(pallet1::Parameters::Key3(pallet1::Key3, Some(123))),
			),
			DispatchError::BadOrigin
		);

		assert_ok!(ModuleParameters::set_parameter(
			Origin::root(),
			Pallet1(pallet1::Parameters::Key3(pallet1::Key3, Some(123))),
		));

		assert_eq!(pallet1::Key3::get(), 123, "Update works");
	});
}

#[test]
fn set_parameters() {
	/*ExtBuilder::new().execute_with(|| {
		assert_eq!(
			<ModuleParameters as RuntimeParameterStore>::get::<pallet1::Parameters, _>(
				pallet1::Key1
			),
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
			<ModuleParameters as RuntimeParameterStore>::get::<pallet1::Parameters, _>(
				pallet1::Key1
			),
			Some(123)
		);

		assert_ok!(ModuleParameters::set_parameter(
			RuntimeOrigin::root(),
			RuntimeParameters::Pallet1(pallet1::Parameters::Key2(pallet1::Key2(234), Some(345))),
		));

		assert_eq!(
			<ModuleParameters as RuntimeParameterStore>::get::<pallet1::Parameters, _>(
				pallet1::Key2(234)
			),
			Some(345)
		);

		assert_eq!(
			<ModuleParameters as RuntimeParameterStore>::get::<pallet1::Parameters, _>(
				pallet1::Key2(235)
			),
			None
		);

		assert_eq!(
			<ModuleParameters as RuntimeParameterStore>::get::<pallet2::Parameters, _>(
				pallet2::Key3((1, 2))
			),
			None
		);

		assert_noop!(
			ModuleParameters::set_parameter(
				RuntimeOrigin::root(),
				RuntimeParameters::Pallet2(pallet2::Parameters::Key3(
					pallet2::Key3((1, 2)),
					Some(123)
				)),
			),
			DispatchError::BadOrigin
		);

		assert_ok!(ModuleParameters::set_parameter(
			RuntimeOrigin::signed(1),
			RuntimeParameters::Pallet2(pallet2::Parameters::Key3(pallet2::Key3((1, 2)), Some(456))),
		));

		assert_eq!(
			<ModuleParameters as RuntimeParameterStore>::get::<pallet2::Parameters, _>(
				pallet2::Key3((1, 2))
			),
			Some(456)
		);
	});*/
}
