#![cfg(test)]

use crate::{
	mock::*, Cell, Error, GamePhase, Games, HoldReason, NextGameId, PlayerGame, PlayerRole,
};
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
fn merkle_cross_verify_with_js() {
	use sp_runtime::traits::BlakeTwo256;

	fn to_hex(bytes: &[u8]) -> String {
		bytes.iter().map(|b| format!("{:02x}", b)).collect()
	}

	fn deterministic_salt(index: u32) -> [u8; 32] {
		let bytes = index.to_le_bytes();
		sp_core::hashing::blake2_256(&bytes)
	}

	let mut cells = Vec::new();
	for i in 0u32..100 {
		cells.push(Cell { salt: deterministic_salt(i), is_occupied: i == 0 });
	}

	let leaves: Vec<[u8; 33]> = cells.iter().map(|c| c.to_leaf()).collect();

	let root = binary_merkle_tree::merkle_root::<BlakeTwo256, _>(leaves.clone());
	let root_hex = to_hex(root.as_ref());
	assert_eq!(
		root_hex, "34d7c679a33c47445dc03c8622a9fbd4397dcbc2a672f11aa07e7cf9303bf81c",
		"Root mismatch between Rust and JS"
	);

	let proof = binary_merkle_tree::merkle_proof::<BlakeTwo256, _, _>(leaves.clone(), 0);
	assert_eq!(proof.proof.len(), 7);

	let expected_proof = [
		"617e1f7bb289c4ead929988093dddda047237be743a6c360c981703f8a77e2e5",
		"bec00da038ac8ac3239bcbedb4d5cb4d54bbfba3f87b20a4940324780f1d857c",
		"c63603894da439608a73e25ea4c03df812adbdb58489c24cb213075d8f6e5399",
		"8b76e713aced70de54cbff9b857714798ff620b7371b1576d6715c7fd252eb74",
		"217c17738a63f3fe04a159606bd77bf645d2c93b48af9836d7d46ea664e29dc8",
		"f632f2207c9ec9772640b82b9fdc5e1d55d0746a9fea3b5936cb4c67a50239e9",
		"2fea6035f5c10cdd0adc0d724eacd5618eb6e16e620a457d7d210c0dee280eba",
	];
	for (i, expected) in expected_proof.iter().enumerate() {
		assert_eq!(to_hex(proof.proof[i].as_ref()), *expected, "Proof element {i} mismatch");
	}

	let valid = binary_merkle_tree::verify_proof::<BlakeTwo256, _, _>(
		&root,
		proof.proof.clone(),
		100,
		0,
		&leaves[0],
	);
	assert!(valid, "Rust verify_proof failed for index 0");

	let proof99 = binary_merkle_tree::merkle_proof::<BlakeTwo256, _, _>(leaves.clone(), 99);
	assert_eq!(proof99.proof.len(), 4);

	let expected_proof99 = [
		"210e04f7eea4e818b30722e3b159c1004cbf2f09abce2dfe97e17e889ca78cc1",
		"29adb6850093ae47f850739a3983ffc30be41746f425ed4547105bf6fcc3f96d",
		"11f3283e95e3ca731674946d1918b57155b6d9ad5a081406d8032291e26b241f",
		"791dd092cc5c6a151803364420689d25f8331b209c8217284266bd461a20aa32",
	];
	for (i, expected) in expected_proof99.iter().enumerate() {
		assert_eq!(to_hex(proof99.proof[i].as_ref()), *expected, "Proof99 element {i} mismatch");
	}

	let valid99 = binary_merkle_tree::verify_proof::<BlakeTwo256, _, _>(
		&root,
		proof99.proof,
		100,
		99,
		&leaves[99],
	);
	assert!(valid99, "Rust verify_proof failed for index 99");
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
			GamePhase::Playing { current_turn: PlayerRole::Player1, pending_attack: None, .. }
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
