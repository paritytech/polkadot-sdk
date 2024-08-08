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
	while frame_system::Pallet::<T>::block_number() < n {
		crate::Pallet::<T>::on_finalize(frame_system::Pallet::<T>::block_number());
		frame_system::Pallet::<T>::on_finalize(frame_system::Pallet::<T>::block_number());
		frame_system::Pallet::<T>::set_block_number(
			frame_system::Pallet::<T>::block_number() + One::one(),
		);
		frame_system::Pallet::<T>::on_initialize(frame_system::Pallet::<T>::block_number());
		crate::Pallet::<T>::on_initialize(frame_system::Pallet::<T>::block_number());
	}
}

fn assert_has_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_has_event(generic_event.into());
}

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

fn create_project<T: Config>(project_account: AccountIdOf<T>, amount: BalanceOf<T>){
    let submission_block = frame_system::Pallet::<T>::block_number();
	let project: types::ProjectInfo<T> =
		ProjectInfo { project_account, submission_block, amount };
	Projects::<T>::mutate(|value| {
		let mut val = value.clone();
		let _ = val.try_push(project);
		*value = val;
	});
}


fn create_parameters<T: Config>(n: u32) -> (AccountIdOf<T>, BalanceOf<T>){
    let project_id = account("project", n, SEED);
	let value: BalanceOf<T> = T::NativeBalance::minimum_balance() * 100u32.into();
    let _ = T::NativeBalance::set_balance(&project_id, value);
    (project_id,value)
}

fn setup_pot_account<T: Config>() {
	let pot_account = Distribution::<T>::pot_account();
	let value = T::NativeBalance::minimum_balance().saturating_mul(1_000_000_000u32.into());
	let _ = T::NativeBalance::set_balance(&pot_account, value);
}


fn add_projects<T: Config>(r:u32) -> Result<(), &'static str> {
    for i in 0..r{
        let (project_id, amount) = create_parameters::<T>(i);
        create_project::<T>(project_id,amount);
    }
    ensure!(<Projects<T>>::get().len() == r as usize, "Not all created");
    Ok(())
}

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn claim_reward_for(r: Linear<1,{T::MaxProjects::get()}>) -> Result<(), BenchmarkError> {
        /* setup initial state */
        add_projects::<T>(r)?;
        setup_pot_account::<T>();
        let (project_id,amount) = create_parameters::<T>(r);
        let caller: T::AccountId = whitelisted_caller();
        let distribution_time = frame_system::Pallet::<T>::block_number() + T::EpochDurationBlocks::get();
        let when = distribution_time+One::one();
        run_to_block::<T>(distribution_time+One::one());
        /* execute extrinsic or function */
        #[extrinsic_call]
        _(RawOrigin::Signed(caller), project_id.clone());
        assert_last_event::<T>(
            Event::RewardClaimed {when, amount, project_account: project_id }.into(),
        );
        ensure!(<Projects<T>>::get().len() == 0 as usize, "Not all rewarded!");
       
		Ok(())
    }
}