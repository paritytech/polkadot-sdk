#![cfg(feature = "runtime-benchmarks")]

use super::*;

use crate::Pallet as Distribution;
use frame_benchmarking::{
	v1::{account, BenchmarkError},
	v2::*,
};
use frame_support::{
	ensure,
	traits::{
		tokens::{ConversionFromAssetBalance, PaymentStatus},
		EnsureOrigin,
	},
};
use frame_system::RawOrigin;
use sp_runtime::traits::One;

const SEED: u32 = 0;

fn run_to_block<T: Config>(n: frame_system::pallet_prelude::BlockNumberFor<T>) {
	while T::BlockNumberProvider::current_block_number() < n {
		crate::Pallet::<T>::on_finalize(T::BlockNumberProvider::current_block_number());
		frame_system::Pallet::<T>::on_finalize(T::BlockNumberProvider::current_block_number());
		frame_system::Pallet::<T>::set_block_number(
			T::BlockNumberProvider::current_block_number() + One::one(),
		);
		frame_system::Pallet::<T>::on_initialize(T::BlockNumberProvider::current_block_number());
		crate::Pallet::<T>::on_initialize(T::BlockNumberProvider::current_block_number());
	}
}

fn create_project<T: Config>(project_account: AccountIdOf<T>, amount: BalanceOf<T>) {
	let submission_block = T::BlockNumberProvider::current_block_number();
	let project: types::ProjectInfo<T> = ProjectInfo { project_account, submission_block, amount };
	Projects::<T>::mutate(|value| {
		let mut val = value.clone();
		let _ = val.try_push(project);
		*value = val;
	});
}

/*fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}*/

fn create_parameters<T: Config>(n: u32) -> (AccountIdOf<T>, BalanceOf<T>) {
	let project_id = account("project", n, SEED);
	let value: BalanceOf<T> = T::NativeBalance::minimum_balance() * 100u32.into() * (n + 1).into();
	let _ = T::NativeBalance::set_balance(&project_id, value);
	(project_id, value)
}

fn setup_pot_account<T: Config>() -> AccountIdOf<T> {
	let pot_account = Distribution::<T>::pot_account();
	let value = T::NativeBalance::minimum_balance().saturating_mul(1_000_000_000u32.into());
	let _ = T::NativeBalance::set_balance(&pot_account, value);
	pot_account
}

fn add_projects<T: Config>(r: u32) -> Result<(), &'static str> {
	for i in 0..r {
		let (project_id, amount) = create_parameters::<T>(i);
		create_project::<T>(project_id, amount);
	}

	Ok(())
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn claim_reward_for() -> Result<(), BenchmarkError> {
		/* setup initial state */
		add_projects::<T>(T::MaxProjects::get())?;

		ensure!(
			<Projects<T>>::get().len() as u32 == T::MaxProjects::get(),
			"Project list setting failed !!"
		);

		let pot = setup_pot_account::<T>();
		let caller: T::AccountId = whitelisted_caller();
		let epoch = T::EpochDurationBlocks::get();
		let mut when = T::BlockNumberProvider::current_block_number().saturating_add(epoch);
		run_to_block::<T>(when);
		/* execute extrinsic or function */
		#[block]
		{
			for i in 0..T::MaxProjects::get() {
				let project = <Spends<T>>::get(i).unwrap();
				when = when.saturating_add(project.valid_from);
				let project_id = project.whitelisted_project.unwrap();
				let amount = project.amount;
				run_to_block::<T>(when);
				let _ = Distribution::<T>::claim_reward_for(
					RawOrigin::Signed(caller.clone()).into(),
					project_id.clone(),
				);

				/*assert_last_event::<T>(
					Event::RewardClaimed { when, amount, project_account: project_id }.into(),
				);*/
			}
		}

		Ok(())
	}

	impl_benchmark_test_suite!(Distribution, crate::mock::new_test_ext(), crate::mock::Test);
}
