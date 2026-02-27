//! Benchmarking setup for pallet-chess

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::v2::*;
use frame_support::traits::fungible::Mutate;
use frame_system::RawOrigin;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn create_game() {
		let caller: T::AccountId = whitelisted_caller();
		let stake = 1000u32.into();

		// Give caller some balance
		T::Currency::set_balance(&caller, 10000u32.into());

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), stake, true, 4, 0);

		// Verify game was created
		let nonce = 0u64;
		let game_id = Pallet::<T>::generate_game_id(&caller, nonce);
		assert!(Games::<T>::contains_key(game_id));
	}

	#[benchmark]
	fn join_game() {
		let creator: T::AccountId = whitelisted_caller();
		let joiner: T::AccountId = account("joiner", 0, 0);
		let stake = 1000u32.into();

		// Give both accounts balance
		T::Currency::set_balance(&creator, 10000u32.into());
		T::Currency::set_balance(&joiner, 10000u32.into());

		// Create game
		let _ = Pallet::<T>::create_game(
			RawOrigin::Signed(creator.clone()).into(),
			stake,
			true,
			4,
			0,
		);

		let game_id = Pallet::<T>::generate_game_id(&creator, 0);

		#[extrinsic_call]
		_(RawOrigin::Signed(joiner), game_id);

		// Verify game was joined
		let game = Games::<T>::get(game_id).unwrap();
		assert!(game.player2.is_some());
	}

	#[benchmark]
	fn submit_move() {
		let player1: T::AccountId = whitelisted_caller();
		let player2: T::AccountId = account("player2", 0, 0);
		let stake = 1000u32.into();

		// Give both accounts balance
		T::Currency::set_balance(&player1, 10000u32.into());
		T::Currency::set_balance(&player2, 10000u32.into());

		// Create and join game
		let _ = Pallet::<T>::create_game(
			RawOrigin::Signed(player1.clone()).into(),
			stake,
			true,
			4,
			0,
		);

		let game_id = Pallet::<T>::generate_game_id(&player1, 0);
		let _ = Pallet::<T>::join_game(RawOrigin::Signed(player2.clone()).into(), game_id);

		#[extrinsic_call]
		_(RawOrigin::Signed(player1), game_id, 12, 28, None);

		// Verify move was made
		let moves = GameMoves::<T>::get(game_id);
		assert_eq!(moves.len(), 1);
	}

	#[benchmark]
	fn resign() {
		let player1: T::AccountId = whitelisted_caller();
		let player2: T::AccountId = account("player2", 0, 0);
		let stake = 1000u32.into();

		// Give both accounts balance
		T::Currency::set_balance(&player1, 10000u32.into());
		T::Currency::set_balance(&player2, 10000u32.into());

		// Create and join game
		let _ = Pallet::<T>::create_game(
			RawOrigin::Signed(player1.clone()).into(),
			stake,
			true,
			4,
			0,
		);

		let game_id = Pallet::<T>::generate_game_id(&player1, 0);
		let _ = Pallet::<T>::join_game(RawOrigin::Signed(player2.clone()).into(), game_id);

		#[extrinsic_call]
		_(RawOrigin::Signed(player1), game_id);

		// Verify game ended
		let game = Games::<T>::get(game_id).unwrap();
		assert_eq!(game.status, GameStatus::Completed);
	}

	#[benchmark]
	fn cancel_game() {
		let caller: T::AccountId = whitelisted_caller();
		let stake = 1000u32.into();

		// Give caller some balance
		T::Currency::set_balance(&caller, 10000u32.into());

		// Create game
		let _ = Pallet::<T>::create_game(
			RawOrigin::Signed(caller.clone()).into(),
			stake,
			true,
			4,
			0,
		);

		let game_id = Pallet::<T>::generate_game_id(&caller, 0);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), game_id);

		// Verify game was cancelled
		let game = Games::<T>::get(game_id).unwrap();
		assert_eq!(game.status, GameStatus::Cancelled);
	}

	#[benchmark]
	fn offer_draw() {
		let player1: T::AccountId = whitelisted_caller();
		let player2: T::AccountId = account("player2", 0, 0);
		let stake = 1000u32.into();

		// Give both accounts balance
		T::Currency::set_balance(&player1, 10000u32.into());
		T::Currency::set_balance(&player2, 10000u32.into());

		// Create and join game
		let _ = Pallet::<T>::create_game(
			RawOrigin::Signed(player1.clone()).into(),
			stake,
			true,
			4,
			0,
		);

		let game_id = Pallet::<T>::generate_game_id(&player1, 0);
		let _ = Pallet::<T>::join_game(RawOrigin::Signed(player2.clone()).into(), game_id);

		#[extrinsic_call]
		_(RawOrigin::Signed(player1.clone()), game_id);

		// Verify draw offer was made
		let game = Games::<T>::get(game_id).unwrap();
		assert_eq!(game.pending_draw_offer, Some(player1));
	}

	#[benchmark]
	fn accept_draw() {
		let player1: T::AccountId = whitelisted_caller();
		let player2: T::AccountId = account("player2", 0, 0);
		let stake = 1000u32.into();

		// Give both accounts balance
		T::Currency::set_balance(&player1, 10000u32.into());
		T::Currency::set_balance(&player2, 10000u32.into());

		// Create and join game
		let _ = Pallet::<T>::create_game(
			RawOrigin::Signed(player1.clone()).into(),
			stake,
			true,
			4,
			0,
		);

		let game_id = Pallet::<T>::generate_game_id(&player1, 0);
		let _ = Pallet::<T>::join_game(RawOrigin::Signed(player2.clone()).into(), game_id);
		let _ = Pallet::<T>::offer_draw(RawOrigin::Signed(player1.clone()).into(), game_id);

		#[extrinsic_call]
		_(RawOrigin::Signed(player2), game_id);

		// Verify game ended in draw
		let game = Games::<T>::get(game_id).unwrap();
		assert_eq!(game.result, GameResult::Draw);
	}

	#[benchmark]
	fn decline_draw() {
		let player1: T::AccountId = whitelisted_caller();
		let player2: T::AccountId = account("player2", 0, 0);
		let stake = 1000u32.into();

		// Give both accounts balance
		T::Currency::set_balance(&player1, 10000u32.into());
		T::Currency::set_balance(&player2, 10000u32.into());

		// Create and join game
		let _ = Pallet::<T>::create_game(
			RawOrigin::Signed(player1.clone()).into(),
			stake,
			true,
			4,
			0,
		);

		let game_id = Pallet::<T>::generate_game_id(&player1, 0);
		let _ = Pallet::<T>::join_game(RawOrigin::Signed(player2.clone()).into(), game_id);
		let _ = Pallet::<T>::offer_draw(RawOrigin::Signed(player1.clone()).into(), game_id);

		#[extrinsic_call]
		_(RawOrigin::Signed(player2), game_id);

		// Verify draw offer was cleared
		let game = Games::<T>::get(game_id).unwrap();
		assert_eq!(game.pending_draw_offer, None);
	}

	impl_benchmark_test_suite!(Pallet, crate::tests::new_test_ext(), crate::tests::Test);
}
