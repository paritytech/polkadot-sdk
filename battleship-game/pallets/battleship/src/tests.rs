#![cfg(test)]

use crate::{mock::*, Error, GamePhase, Games, HoldReason, NextGameId, PlayerGame, PlayerRole};
use frame_support::{
	assert_noop, assert_ok,
	traits::{
		fungible::{Inspect, InspectHold, MutateHold},
		tokens::Precision,
		Hooks,
	},
	weights::Weight,
};
use sp_core::H256;

fn total_balance(who: &u64) -> u64 {
	<Balances as Inspect<u64>>::balance(who) +
		<Balances as InspectHold<u64>>::total_balance_on_hold(who)
}

fn create_game_with_players() -> (u64, u64, u64) {
	let player1 = 1u64;
	let player2 = 2u64;
	let pot = 1000u64;

	assert_ok!(Battleship::create_game(RuntimeOrigin::signed(player1), pot));
	let game_id = NextGameId::<Test>::get() - 1;
	assert_ok!(Battleship::join_game(RuntimeOrigin::signed(player2), game_id));

	(game_id, player1, player2)
}

fn setup_game_in_playing_phase() -> (u64, u64, u64) {
	let (game_id, player1, player2) = create_game_with_players();

	let grid_root = H256::repeat_byte(0x01);
	assert_ok!(Battleship::commit_grid(RuntimeOrigin::signed(player1), game_id, grid_root));
	assert_ok!(Battleship::commit_grid(RuntimeOrigin::signed(player2), game_id, grid_root));

	(game_id, player1, player2)
}

fn run_on_idle() {
	Battleship::on_idle(System::block_number(), Weight::MAX);
}

#[test]
fn create_game_works() {
	new_test_ext().execute_with(|| {
		let player1 = 1u64;
		let pot = 1000u64;

		assert_ok!(Battleship::create_game(RuntimeOrigin::signed(player1), pot));

		let game_id = NextGameId::<Test>::get() - 1;
		let game = Games::<Test>::get(game_id).unwrap();
		assert_eq!(game.player1, player1);
		assert_eq!(game.pot_amount, pot);
		assert_eq!(game.phase, GamePhase::WaitingForOpponent);
		assert!(PlayerGame::<Test>::contains_key(&player1));
	});
}

#[test]
fn join_game_works() {
	new_test_ext().execute_with(|| {
		let (game_id, player1, player2) = create_game_with_players();

		let game = Games::<Test>::get(game_id).unwrap();
		assert_eq!(game.player2, Some(player2));
		assert!(matches!(game.phase, GamePhase::Setup { .. }));
		assert!(PlayerGame::<Test>::contains_key(&player1));
		assert!(PlayerGame::<Test>::contains_key(&player2));
	});
}

#[test]
fn cannot_join_own_game() {
	new_test_ext().execute_with(|| {
		let player1 = 1u64;
		let pot = 1000u64;

		assert_ok!(Battleship::create_game(RuntimeOrigin::signed(player1), pot));
		let game_id = NextGameId::<Test>::get() - 1;

		assert_noop!(
			Battleship::join_game(RuntimeOrigin::signed(player1), game_id),
			Error::<Test>::PlayerAlreadyInGame
		);
	});
}

#[test]
fn commit_grid_transitions_to_playing() {
	new_test_ext().execute_with(|| {
		let (game_id, player1, player2) = create_game_with_players();

		let grid_root = H256::repeat_byte(0x01);
		assert_ok!(Battleship::commit_grid(RuntimeOrigin::signed(player1), game_id, grid_root));

		let game = Games::<Test>::get(game_id).unwrap();
		assert!(matches!(
			game.phase,
			GamePhase::Setup { player1_ready: true, player2_ready: false }
		));

		assert_ok!(Battleship::commit_grid(RuntimeOrigin::signed(player2), game_id, grid_root));

		let game = Games::<Test>::get(game_id).unwrap();
		assert!(matches!(
			game.phase,
			GamePhase::Playing { current_turn: PlayerRole::Player1, pending_attack: None }
		));
	});
}

#[test]
fn surrender_works() {
	new_test_ext().execute_with(|| {
		let (game_id, player1, player2) = setup_game_in_playing_phase();

		let p1_balance_before = total_balance(&player1);
		let p2_balance_before = total_balance(&player2);

		assert_ok!(Battleship::surrender(RuntimeOrigin::signed(player1), game_id));

		assert!(Games::<Test>::get(game_id).is_none());
		assert!(!PlayerGame::<Test>::contains_key(&player1));
		assert!(!PlayerGame::<Test>::contains_key(&player2));

		let p1_balance_after = total_balance(&player1);
		let p2_balance_after = total_balance(&player2);
		assert!(p2_balance_after > p2_balance_before);
		assert!(p1_balance_after < p1_balance_before);
	});
}

#[test]
fn abort_abandoned_game_with_missing_hold_succeeds() {
	new_test_ext().execute_with(|| {
		let player1 = 1u64;
		let player2 = 2u64;
		let pot = 1000u64;

		assert_ok!(Battleship::create_game(RuntimeOrigin::signed(player1), pot));
		let game_id = NextGameId::<Test>::get() - 1;
		assert_ok!(Battleship::join_game(RuntimeOrigin::signed(player2), game_id));

		let game = Games::<Test>::get(game_id).unwrap();
		assert!(matches!(game.phase, GamePhase::Setup { .. }));

		let _ = <Balances as MutateHold<u64>>::release(
			&HoldReason::GamePot.into(),
			&player1,
			pot,
			Precision::BestEffort,
		);

		let held =
			<Balances as InspectHold<u64>>::balance_on_hold(&HoldReason::GamePot.into(), &player1);
		assert_eq!(held, 0);

		System::set_block_number(1 + 960 + 1);

		run_on_idle();

		assert!(Games::<Test>::get(game_id).is_none());

		assert!(!PlayerGame::<Test>::contains_key(&player1));
		assert!(!PlayerGame::<Test>::contains_key(&player2));
	});
}

#[test]
fn abort_abandoned_game_with_partial_hold_succeeds() {
	new_test_ext().execute_with(|| {
		let player1 = 1u64;
		let player2 = 2u64;
		let pot = 1000u64;

		assert_ok!(Battleship::create_game(RuntimeOrigin::signed(player1), pot));
		let game_id = NextGameId::<Test>::get() - 1;
		assert_ok!(Battleship::join_game(RuntimeOrigin::signed(player2), game_id));

		let _ = <Balances as MutateHold<u64>>::release(
			&HoldReason::GamePot.into(),
			&player1,
			pot,
			Precision::BestEffort,
		);

		let held1 =
			<Balances as InspectHold<u64>>::balance_on_hold(&HoldReason::GamePot.into(), &player1);
		let held2 =
			<Balances as InspectHold<u64>>::balance_on_hold(&HoldReason::GamePot.into(), &player2);
		assert_eq!(held1, 0);
		assert_eq!(held2, pot);

		System::set_block_number(1 + 960 + 1);

		run_on_idle();

		assert!(Games::<Test>::get(game_id).is_none());

		assert!(!PlayerGame::<Test>::contains_key(&player1));
		assert!(!PlayerGame::<Test>::contains_key(&player2));

		let held2_after =
			<Balances as InspectHold<u64>>::balance_on_hold(&HoldReason::GamePot.into(), &player2);
		assert_eq!(held2_after, 0);
	});
}

#[test]
fn abort_abandoned_game_with_normal_hold_burns_funds() {
	new_test_ext().execute_with(|| {
		let player1 = 1u64;
		let player2 = 2u64;
		let pot = 1000u64;

		let p1_initial = total_balance(&player1);
		let p2_initial = total_balance(&player2);

		assert_ok!(Battleship::create_game(RuntimeOrigin::signed(player1), pot));
		let game_id = NextGameId::<Test>::get() - 1;
		assert_ok!(Battleship::join_game(RuntimeOrigin::signed(player2), game_id));

		System::set_block_number(1 + 960 + 1);

		run_on_idle();

		assert!(Games::<Test>::get(game_id).is_none());

		let p1_final = total_balance(&player1);
		let p2_final = total_balance(&player2);
		assert_eq!(p1_final, p1_initial - pot);
		assert_eq!(p2_final, p2_initial - pot);
	});
}

#[test]
fn active_game_not_aborted() {
	new_test_ext().execute_with(|| {
		let (game_id, player1, player2) = setup_game_in_playing_phase();

		System::set_block_number(500);

		run_on_idle();

		assert!(Games::<Test>::get(game_id).is_some());
		assert!(PlayerGame::<Test>::contains_key(&player1));
		assert!(PlayerGame::<Test>::contains_key(&player2));
	});
}
