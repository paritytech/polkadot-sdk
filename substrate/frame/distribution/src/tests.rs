pub use super::*;
use crate::mock::*;
use frame_support::{assert_noop, assert_ok};

pub fn next_block() {
	System::set_block_number(System::block_number() + 1);
	AllPalletsWithSystem::on_initialize(System::block_number());
}

pub fn run_to_block(n: BlockNumberFor<Test>) {
	while System::block_number() < n {
		if System::block_number() > 1 {
			AllPalletsWithSystem::on_finalize(System::block_number());
		}
		next_block();
	}
}

pub fn create_project(project_account: AccountId, amount: u128){
	let whitelisted_block = System::block_number();
	let project: types::ProjectInfo<Test>  = ProjectInfo {project_account, whitelisted_block,amount};
	Projects::<Test>::mutate(|value|{
		let mut val = value.clone();
		let _= val.try_push(project);
		*value = val;

	});

}

#[test]
fn spendings_creation_works() {
	new_test_ext().execute_with( || {
		// Add 3 projects
		let amount1 = 1_000_000 * BSX;
		let amount2 = 1_200_000 * BSX;
		let amount3 = 2_000_000 * BSX;
		create_project(ALICE, amount1);
		create_project(BOB, amount2);
		create_project(DAVE, amount3);

		// The Spendings Storage should be empty
		assert_eq!(SpendingsCount::<Test>::get(), 0);
		

		// Move to epoch block => Warning: We set the system block at 1 in mock.rs, so now = Epoch_Block + 1 
		let now =
			System::block_number().saturating_add(<Test as Config>::EpochDurationBlocks::get().into());
			run_to_block(now);


		
		// We should have 3 spendings
		assert!(SpendingsCount::<Test>::get() == 3);

		// The 3 spendings are known
		let alice_spending: types::SpendingInfo<Test> = SpendingInfo{
			amount: amount1,
			valid_from: now,
			status: types::SpendingState::default(),
			whitelisted_project: Some(ALICE),
			claimed: false,
		};

		let bob_spending: types::SpendingInfo<Test> = SpendingInfo{
			amount: amount2,
			valid_from: now,
			status: types::SpendingState::default(),
			whitelisted_project: Some(BOB),
			claimed: false,
		};

		let dave_spending: types::SpendingInfo<Test> = SpendingInfo{
			amount: amount3,
			valid_from: now,
			status: types::SpendingState::default(),
			whitelisted_project: Some(DAVE),
			claimed: false,
		};

		// List of spendings actually created & stored 
		let list0:Vec<_> = Spendings::<Test>::iter_keys().collect();
		let list:Vec<_> = list0.into_iter().map(|x| Spendings::<Test>::get(x)).collect();
		
		expect_events(vec![
		RuntimeEvent::Distribution(Event::SpendingCreated{
			when: now.saturating_sub(1),
			amount: list[0].clone().unwrap().amount,
			project_account: list[0].clone().unwrap().whitelisted_project.unwrap(),
		}),
		RuntimeEvent::Distribution(Event::SpendingCreated{
			when: now.saturating_sub(1),
			amount: list[1].clone().unwrap().amount,
			project_account: list[1].clone().unwrap().whitelisted_project.unwrap(),
		}),
		RuntimeEvent::Distribution(Event::SpendingCreated{
			when: now.saturating_sub(1),
			amount: list[2].clone().unwrap().amount,
			project_account: list[2].clone().unwrap().whitelisted_project.unwrap(),
		}),
		]);

		assert!(list.contains(&Some(alice_spending)));
		assert!(list.contains(&Some(bob_spending)));
		assert!(list.contains(&Some(dave_spending)));
		


	})
}

#[test]
fn funds_are_locked() {
	new_test_ext().execute_with( || {
		// Add 3 projects
		let amount1 = 1_000_000 * BSX;
		let amount2 = 1_200_000 * BSX;
		let amount3 = 2_000_000 * BSX;
		create_project(ALICE, amount1);
		create_project(BOB, amount2);
		create_project(DAVE, amount3);

		// The Spendings Storage should be empty
		assert_eq!(SpendingsCount::<Test>::get(), 0);
		

		// Move to epoch block => Warning: We set the system block at 1 in mock.rs, so now = Epoch_Block + 1 
		let now =
			System::block_number().saturating_add(<Test as Config>::EpochDurationBlocks::get().into());
		run_to_block(now);

		let total_on_hold = amount1.saturating_add(amount2).saturating_add(amount3);
		let pot_account = Distribution::pot_account();
		let hold = <<Test as Config>::NativeBalance as fungible::hold::Inspect<u64>>::balance_on_hold(
			&HoldReason::FundsLock.into(),
			&pot_account
		);
		assert_eq!(total_on_hold, hold);	
		
	})
}

#[test]
fn funds_claim_works() {
	new_test_ext().execute_with( || {
		// Add 3 projects
		let amount1 = 1_000_000 * BSX;
		let amount2 = 1_200_000 * BSX;
		let amount3 = 2_000_000 * BSX;
		create_project(ALICE, amount1);
		create_project(BOB, amount2);
		create_project(DAVE, amount3);

		// The Spendings Storage should be empty
		assert_eq!(SpendingsCount::<Test>::get(), 0);
		

		// Move to epoch block => Warning: We set the system block at 1 in mock.rs, so now = Epoch_Block + 1 
		let mut now =
			System::block_number().saturating_add(<Test as Config>::EpochDurationBlocks::get().into());
		run_to_block(now);


		let project = Spendings::<Test>::get(0).unwrap();
		let project_account = project.whitelisted_project.unwrap();
		let balance_0 = <<Test as Config>::NativeBalance as fungible::Inspect<u64>>::balance(&project_account);
		now = now.saturating_add(project.valid_from);
		run_to_block(now);

		assert_ok!(Distribution::claim_reward_for(
			RawOrigin::Signed(EVE).into(),
			project_account.clone(),
		));
		let balance_1 = <<Test as Config>::NativeBalance as fungible::Inspect<u64>>::balance(&project_account);
		
		assert!(balance_1 > balance_0);

	})
}

#[test]
fn funds_claim_fails_before_claim_period() {
	new_test_ext().execute_with( || {
		// Add 3 projects
		let amount1 = 1_000_000 * BSX;
		let amount2 = 1_200_000 * BSX;
		let amount3 = 2_000_000 * BSX;
		create_project(ALICE, amount1);
		create_project(BOB, amount2);
		create_project(DAVE, amount3);

		// The Spendings Storage should be empty
		assert_eq!(SpendingsCount::<Test>::get(), 0);
		

		// Move to epoch block => Warning: We set the system block at 1 in mock.rs, so now = Epoch_Block + 1 
		let now =
			System::block_number().saturating_add(<Test as Config>::EpochDurationBlocks::get().into());
		run_to_block(now);


		let project = Spendings::<Test>::get(0).unwrap();
		let project_account = project.whitelisted_project.unwrap();

		assert_noop!(Distribution::claim_reward_for(
			RawOrigin::Signed(EVE).into(),
			project_account.clone(),
		), Error::<Test>::NotClaimingPeriod);
		
	})
}