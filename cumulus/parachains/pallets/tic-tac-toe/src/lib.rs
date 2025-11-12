// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! # Tic-Tac-Toe Pallet
//!
//! A pallet that implements a tic-tac-toe game with matchmaking.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	pallet_prelude::IsType,
	traits::fungible::{Inspect, Mutate},
};
use frame_system::pallet_prelude::*;
use scale_info::TypeInfo;
use sp_runtime::{
	transaction_validity::{
		InvalidTransaction, TransactionLongevity, TransactionSource, TransactionValidity,
		ValidTransaction,
	},
	ArithmeticError,
};

extern crate alloc;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::ensure_none;
	use sp_runtime::traits::BlockNumberProvider;

	use sp_runtime::traits::ValidateUnsigned;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Timeout in relaychain blocks (5 blocks = ~30 seconds at 6s per block)
	pub const MOVE_TIMEOUT_BLOCKS: u32 = 5;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The currency trait for handling balances
		type Currency: Inspect<Self::AccountId> + Mutate<Self::AccountId>;

		/// Provides relay chain block numbers for timeout measurements
		type RcBlockNumberProvider: sp_runtime::traits::BlockNumberProvider<BlockNumber = u32>;
	}

	/// Represents a cell on the board
	#[derive(Clone, Copy, Encode, Decode, TypeInfo, MaxEncodedLen, PartialEq, Eq, Debug)]
	pub enum Cell {
		Empty,
		X,
		O,
	}

	impl Default for Cell {
		fn default() -> Self {
			Cell::Empty
		}
	}

	/// Game state
	#[derive(Clone, Copy, Encode, Decode, TypeInfo, MaxEncodedLen, PartialEq, Eq, Debug)]
	pub enum GameState {
		InProgress,
		XWon,
		OWon,
		Draw,
	}

	impl GameState {
		fn as_u8(self) -> u8 {
			match self {
				GameState::InProgress => 0,
				GameState::XWon => 1,
				GameState::OWon => 2,
				GameState::Draw => 3,
			}
		}
	}

	/// Game information
	#[derive(Clone, Encode, Decode, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	pub struct Game<T: Config> {
		/// Player X
		pub player_x: T::AccountId,
		/// Player O
		pub player_o: T::AccountId,
		/// Current turn (true = X, false = O)
		pub x_turn: bool,
		/// Board state (3x3 grid, stored as 9 cells)
		pub board: [Cell; 9],
		/// Game state
		pub state: GameState,
		/// Relaychain block number of the last move
		pub last_move_block: u32,
	}

	/// Storage for active games, indexed by game ID
	#[pallet::storage]
	#[pallet::getter(fn games)]
	pub type Games<T: Config> = StorageMap<_, Blake2_128Concat, u32, Game<T>>;

	/// Next game ID
	#[pallet::storage]
	#[pallet::getter(fn next_game_id)]
	pub type NextGameId<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// Matchmaking queue - stores players waiting for a game
	#[pallet::storage]
	#[pallet::getter(fn matchmaking_queue)]
	pub type MatchmakingQueue<T: Config> =
		StorageValue<_, BoundedVec<T::AccountId, ConstU32<100>>, ValueQuery>;

	/// Maps each player to their active game ID (only one active game per player)
	#[pallet::storage]
	#[pallet::getter(fn player_games)]
	pub type PlayerGames<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, u32>;

	/// Events
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new game was created [game_id, player_x, player_o]
		GameCreated { game_id: u32, player_x: T::AccountId, player_o: T::AccountId },
		/// A move was made [game_id, player, position]
		MoveMade { game_id: u32, player: T::AccountId, position: u8 },
		/// Game ended [game_id, state_u8]
		GameEnded { game_id: u32, state_u8: u8 },
		/// A player joined the matchmaking queue [player]
		PlayerJoinedQueue { player: T::AccountId },
		/// A player left the matchmaking queue [player]
		PlayerLeftQueue { player: T::AccountId },
		/// Prize funds were transferred [from, to, amount]
		PrizeTransferred { from: T::AccountId, to: T::AccountId, amount: BalanceOf<T> },
		/// Funds were minted to an account [account, amount]
		FundsMinted { account: T::AccountId, amount: BalanceOf<T> },
		/// Game was claimed due to timeout [game_id, winner, loser]
		GameClaimedByTimeout { game_id: u32, winner: T::AccountId, loser: T::AccountId },
	}

	pub type BalanceOf<T> =
		<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

	/// Errors
	#[pallet::error]
	pub enum Error<T> {
		/// Game does not exist
		GameNotFound,
		/// Not your turn
		NotYourTurn,
		/// Invalid position (must be 0-8)
		InvalidPosition,
		/// Cell already occupied
		CellOccupied,
		/// Game already ended
		GameEnded,
		/// Cannot play against yourself
		CannotPlayAgainstSelf,
		/// You are not a player in this game
		NotAPlayer,
		/// Already in matchmaking queue
		AlreadyInQueue,
		/// Not in matchmaking queue
		NotInQueue,
		/// Insufficient funds
		InsufficientFunds,
		/// Matchmaking queue is full
		QueueFull,
		/// Player already has an active game
		AlreadyInGame,
		/// Move timeout has not elapsed yet
		TimeoutNotElapsed,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Make a move
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().reads_writes(1, 1))]
		pub fn make_move(origin: OriginFor<T>, game_id: u32, position: u8) -> DispatchResult {
			let who = ensure_signed(origin)?;

			log::info!(
				target: "runtime::tic-tac-toe",
				"Player {:?} making move in game {} at position {}",
				who,
				game_id,
				position
			);

			// Validate position
			ensure!(position < 9, Error::<T>::InvalidPosition);

			Games::<T>::try_mutate(game_id, |maybe_game| -> DispatchResult {
				let game = maybe_game.as_mut().ok_or(Error::<T>::GameNotFound)?;

				log::debug!(
					target: "runtime::tic-tac-toe",
					"Game {} state before move - X's turn: {}, state: {:?}",
					game_id,
					game.x_turn,
					game.state
				);

				// Check game is still in progress
				ensure!(game.state == GameState::InProgress, Error::<T>::GameEnded);

				// Check if it's the player's turn
				let is_player_x = who == game.player_x;
				let is_player_o = who == game.player_o;
				ensure!(is_player_x || is_player_o, Error::<T>::NotAPlayer);

				if game.x_turn {
					ensure!(is_player_x, Error::<T>::NotYourTurn);
				} else {
					ensure!(is_player_o, Error::<T>::NotYourTurn);
				}

				// Check if cell is empty
				let pos = position as usize;
				ensure!(game.board[pos] == Cell::Empty, Error::<T>::CellOccupied);

				// Make the move
				let piece = if game.x_turn { Cell::X } else { Cell::O };
				game.board[pos] = piece;

				// Update last move block number using relay chain block number provider
				let current_relay_block = T::RcBlockNumberProvider::current_block_number();
				game.last_move_block = current_relay_block;

				log::debug!(
					target: "runtime::tic-tac-toe",
					"Placed {:?} at position {} in game {} at relay block {}",
					piece,
					position,
					game_id,
					current_relay_block
				);

				Self::deposit_event(Event::MoveMade { game_id, player: who, position });

				// Check for win or draw
				game.state = Self::check_game_state(&game.board);

				if game.state != GameState::InProgress {
					log::info!(
						target: "runtime::tic-tac-toe",
						"Game {} ended with state: {:?}",
						game_id,
						game.state
					);

					Self::deposit_event(Event::GameEnded { game_id, state_u8: game.state.as_u8() });

					// Transfer 50% of loser's funds to winner
					match game.state {
						GameState::XWon => {
							// Player X won, take 50% from Player O
							let loser = game.player_o.clone();
							let winner = game.player_x.clone();
							Self::transfer_prize(&loser, &winner);
						},
						GameState::OWon => {
							// Player O won, take 50% from Player X
							let loser = game.player_x.clone();
							let winner = game.player_o.clone();
							Self::transfer_prize(&loser, &winner);
						},
						GameState::Draw => {
							// No transfer on draw
							log::info!(
								target: "runtime::tic-tac-toe",
								"Game {} ended in a draw, no funds transferred",
								game_id
							);
						},
						GameState::InProgress => {
							// Should never happen
							unreachable!();
						},
					}

					return Ok(());
				}

				// Switch turn
				game.x_turn = !game.x_turn;

				log::trace!(
					target: "runtime::tic-tac-toe",
					"Game {} board state: {:?}",
					game_id,
					game.board
				);

				Ok(())
			})
		}

		/// Join matchmaking queue to find an opponent
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().reads_writes(2, 3))]
		pub fn play_game(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			log::info!(
				target: "runtime::tic-tac-toe",
				"Player {:?} joining matchmaking queue",
				who
			);

			MatchmakingQueue::<T>::try_mutate(|queue| -> DispatchResult {
				// Check if player is already in queue
				ensure!(!queue.contains(&who), Error::<T>::AlreadyInQueue);

				// Ensure player doesn't already have an active game
				ensure!(!Self::has_active_game(&who), Error::<T>::AlreadyInGame);

				// Check if there's someone waiting in the queue
				if let Some(opponent) = queue.pop() {
					// Ensure player is not playing against themselves
					ensure!(who != opponent, Error::<T>::CannotPlayAgainstSelf);

					// Ensure opponent doesn't already have an active game
					ensure!(!Self::has_active_game(&opponent), Error::<T>::AlreadyInGame);

					// Create a new game
					let game_id = NextGameId::<T>::get();
					let next_id = game_id.checked_add(1).ok_or(ArithmeticError::Overflow)?;

					log::info!(
						target: "runtime::tic-tac-toe",
						"Matchmaking successful - creating game {} between {:?} and {:?}",
						game_id,
						opponent,
						who
					);

					// Get current relaychain block number using relay chain block number provider
					let current_relay_block = T::RcBlockNumberProvider::current_block_number();

					let game = Game {
						player_x: opponent.clone(),
						player_o: who.clone(),
						x_turn: true,
						board: [Cell::Empty; 9],
						state: GameState::InProgress,
						last_move_block: current_relay_block,
					};

					Games::<T>::insert(game_id, game);
					NextGameId::<T>::put(next_id);

					// Track both players in PlayerGames
					PlayerGames::<T>::insert(&opponent, game_id);
					PlayerGames::<T>::insert(&who, game_id);

					Self::deposit_event(Event::GameCreated {
						game_id,
						player_x: opponent,
						player_o: who,
					});
				} else {
					// Add player to queue
					queue.try_push(who.clone()).map_err(|_| Error::<T>::QueueFull)?;
					log::info!(
						target: "runtime::tic-tac-toe",
						"Player {:?} added to matchmaking queue",
						who
					);
					Self::deposit_event(Event::PlayerJoinedQueue { player: who });
				}

				Ok(())
			})
		}

		/// Cancel matchmaking and leave the queue
		#[pallet::call_index(2)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().reads_writes(1, 1))]
		pub fn cancel_matchmaking(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			log::info!(
				target: "runtime::tic-tac-toe",
				"Player {:?} canceling matchmaking",
				who
			);

			MatchmakingQueue::<T>::try_mutate(|queue| -> DispatchResult {
				// Find and remove player from queue
				if let Some(pos) = queue.iter().position(|p| p == &who) {
					queue.remove(pos);
					log::info!(
						target: "runtime::tic-tac-toe",
						"Player {:?} removed from matchmaking queue",
						who
					);
					Self::deposit_event(Event::PlayerLeftQueue { player: who });
					Ok(())
				} else {
					Err(Error::<T>::NotInQueue.into())
				}
			})
		}

		/// Mint funds to an account (no fee required)
		#[pallet::call_index(3)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn mint_funds(
			origin: OriginFor<T>,
			dest: T::AccountId,
			amount: BalanceOf<T>,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;

			T::Currency::mint_into(&dest, amount)?;
			log::info!(
				target: "runtime::tic-tac-toe",
				"Minting {:?} funds to {:?}",
				amount,
				dest
			);

			Self::deposit_event(Event::FundsMinted { account: dest, amount });

			Ok(Pays::No.into())
		}

		/// Claim victory due to opponent timeout
		#[pallet::call_index(4)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().reads_writes(1, 1))]
		pub fn claim_timeout(origin: OriginFor<T>, game_id: u32) -> DispatchResult {
			let who = ensure_signed(origin)?;

			log::info!(
				target: "runtime::tic-tac-toe",
				"Player {:?} claiming timeout for game {}",
				who,
				game_id
			);

			// Get current relaychain block number using relay chain block number provider
			let current_relay_block = T::RcBlockNumberProvider::current_block_number();

			Games::<T>::try_mutate(game_id, |maybe_game| -> DispatchResult {
				let game = maybe_game.as_mut().ok_or(Error::<T>::GameNotFound)?;

				// Check game is still in progress
				ensure!(game.state == GameState::InProgress, Error::<T>::GameEnded);

				// Check if caller is a player
				let is_player_x = who == game.player_x;
				let is_player_o = who == game.player_o;
				ensure!(is_player_x || is_player_o, Error::<T>::NotAPlayer);

				// Check if it's NOT the caller's turn
				let is_caller_turn = (is_player_x && game.x_turn) || (is_player_o && !game.x_turn);
				ensure!(!is_caller_turn, Error::<T>::NotYourTurn);

				// Check if timeout has elapsed
				let blocks_since_last_move =
					current_relay_block.saturating_sub(game.last_move_block);
				ensure!(
					blocks_since_last_move >= MOVE_TIMEOUT_BLOCKS,
					Error::<T>::TimeoutNotElapsed
				);

				log::info!(
					target: "runtime::tic-tac-toe",
					"Timeout claim successful for game {} - {} blocks since last move",
					game_id,
					blocks_since_last_move
				);

				// Determine winner and loser
				let (winner, loser) = if is_player_x {
					(game.player_x.clone(), game.player_o.clone())
				} else {
					(game.player_o.clone(), game.player_x.clone())
				};

				// Set game state based on who won
				game.state = if is_player_x { GameState::XWon } else { GameState::OWon };

				Self::deposit_event(Event::GameEnded { game_id, state_u8: game.state.as_u8() });
				Self::deposit_event(Event::GameClaimedByTimeout {
					game_id,
					winner: winner.clone(),
					loser: loser.clone(),
				});

				// Transfer 50% of loser's funds to winner
				Self::transfer_prize(&loser, &winner);

				Ok(())
			})
		}
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;

		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			use crate::pallet::alloc::vec;
			const PRIORITY: u64 = 100;

			let dest = match call {
				Call::mint_funds { dest, amount: _ } => dest,
				_ => return Err(InvalidTransaction::Call.into()),
			};

			Ok(ValidTransaction {
				priority: PRIORITY,
				requires: vec![],
				provides: vec![dest.encode()],
				longevity: TransactionLongevity::from(100u64),
				propagate: true,
			})
		}
	}

	impl<T: Config> Pallet<T> {
		/// Check if a player has an active game (InProgress)
		pub fn has_active_game(player: &T::AccountId) -> bool {
			if let Some(game_id) = PlayerGames::<T>::get(player) {
				if let Some(game) = Games::<T>::get(game_id) {
					return game.state == GameState::InProgress;
				}
			}
			false
		}

		/// Get the game for a player (returns the game regardless of state)
		pub fn get_player_game(player: &T::AccountId) -> Option<(u32, Game<T>)> {
			// Look up the player's game in PlayerGames
			if let Some(game_id) = PlayerGames::<T>::get(player) {
				if let Some(game) = Games::<T>::get(game_id) {
					return Some((game_id, game));
				}
			}
			None
		}

		/// Check the current game state
		fn check_game_state(board: &[Cell; 9]) -> GameState {
			// Check all possible winning combinations
			let winning_combinations = [
				// Rows
				[0, 1, 2],
				[3, 4, 5],
				[6, 7, 8],
				// Columns
				[0, 3, 6],
				[1, 4, 7],
				[2, 5, 8],
				// Diagonals
				[0, 4, 8],
				[2, 4, 6],
			];

			for combo in &winning_combinations {
				let [a, b, c] = *combo;
				if board[a] != Cell::Empty && board[a] == board[b] && board[b] == board[c] {
					return match board[a] {
						Cell::X => GameState::XWon,
						Cell::O => GameState::OWon,
						Cell::Empty => unreachable!(),
					};
				}
			}

			// Check for draw (no empty cells)
			if board.iter().all(|cell| *cell != Cell::Empty) {
				return GameState::Draw;
			}

			GameState::InProgress
		}

		/// Transfer 50% of loser's funds to winner
		fn transfer_prize(loser: &T::AccountId, winner: &T::AccountId) {
			use frame_support::traits::fungible::Inspect;
			use sp_runtime::traits::{CheckedDiv, Zero};

			let loser_balance = T::Currency::balance(loser);

			// Calculate 50% of loser's balance
			if let Some(prize) = loser_balance.checked_div(&2u32.into()) {
				if !prize.is_zero() {
					// Try to transfer the prize
					match T::Currency::transfer(
						loser,
						winner,
						prize,
						frame_support::traits::tokens::Preservation::Expendable,
					) {
						Ok(_) => {
							log::info!(
								target: "runtime::tic-tac-toe",
								"Transferred {:?} funds from {:?} to {:?}",
								prize,
								loser,
								winner
							);

							Self::deposit_event(Event::PrizeTransferred {
								from: loser.clone(),
								to: winner.clone(),
								amount: prize,
							});
						},
						Err(e) => {
							log::warn!(
								target: "runtime::tic-tac-toe",
								"Failed to transfer prize from {:?} to {:?}: {:?}",
								loser,
								winner,
								e
							);
						},
					}
				} else {
					log::info!(
						target: "runtime::tic-tac-toe",
						"Loser {:?} has no funds to transfer",
						loser
					);
				}
			}
		}
	}
}

pub use pallet::*;

sp_api::decl_runtime_apis! {
	/// Runtime API for querying tic-tac-toe games
	pub trait TicTacToeApi<AccountId> where
		AccountId: codec::Codec,
	{
		/// Get the active game for a player (returns None if player has no active game)
		fn get_player_game(player: AccountId) -> Option<(u32, Game<AccountId>)>;
	}
}

/// Simplified Game struct for runtime API that doesn't depend on Config trait
#[derive(Clone, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct Game<AccountId> {
	/// Player X
	pub player_x: AccountId,
	/// Player O
	pub player_o: AccountId,
	/// Current turn (true = X, false = O)
	pub x_turn: bool,
	/// Board state (3x3 grid, stored as 9 cells)
	pub board: [Cell; 9],
	/// Game state
	pub state: GameState,
	/// Relaychain block number of the last move
	pub last_move_block: u32,
}

impl<AccountId> Game<AccountId> {
	/// Convert from pallet::Game to runtime API Game
	pub fn from_pallet_game<T: pallet::Config<AccountId = AccountId>>(
		game: pallet::Game<T>,
	) -> Self {
		Self {
			player_x: game.player_x,
			player_o: game.player_o,
			x_turn: game.x_turn,
			board: game.board,
			state: game.state,
			last_move_block: game.last_move_block,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use frame_support::{assert_err, assert_ok, derive_impl, parameter_types, traits::ConstU32};
	use sp_runtime::{traits::IdentityLookup, BuildStorage};

	type Block = frame_system::mocking::MockBlock<Test>;

	// Configure a mock runtime to test the pallet.
	frame_support::construct_runtime!(
		pub enum Test {
			System: frame_system,
			Balances: pallet_balances,
			TicTacToe: pallet,
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Test {
		type Block = Block;
		type AccountId = u64;
		type Lookup = IdentityLookup<Self::AccountId>;
		type AccountData = pallet_balances::AccountData<u128>;
	}

	parameter_types! {
		pub const ExistentialDeposit: u128 = 1;
	}

	impl pallet_balances::Config for Test {
		type MaxLocks = ConstU32<50>;
		type MaxReserves = ConstU32<50>;
		type ReserveIdentifier = [u8; 8];
		type Balance = u128;
		type RuntimeEvent = RuntimeEvent;
		type DustRemoval = ();
		type ExistentialDeposit = ExistentialDeposit;
		type AccountStore = System;
		type WeightInfo = ();
		type FreezeIdentifier = ();
		type MaxFreezes = ConstU32<0>;
		type RuntimeHoldReason = RuntimeHoldReason;
		type RuntimeFreezeReason = RuntimeFreezeReason;
		type DoneSlashHandler = ();
	}

	// Simple relay chain block number provider for tests
	pub struct TestRcBlockNumberProvider;
	impl sp_runtime::traits::BlockNumberProvider for TestRcBlockNumberProvider {
		type BlockNumber = u32;

		fn current_block_number() -> Self::BlockNumber {
			System::block_number() as u32
		}
	}

	impl pallet::Config for Test {
		type RuntimeEvent = RuntimeEvent;
		type Currency = Balances;
		type RcBlockNumberProvider = TestRcBlockNumberProvider;
	}

	// Build genesis storage according to the mock runtime.
	pub fn new_test_ext() -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

		pallet_balances::GenesisConfig::<Test> {
			balances: vec![(1, 1000), (2, 1000), (3, 1000), (4, 1000)],
			dev_accounts: None,
		}
		.assimilate_storage(&mut t)
		.unwrap();

		let mut ext = sp_io::TestExternalities::new(t);
		ext.execute_with(|| System::set_block_number(1));
		ext
	}

	#[test]
	fn test_get_player_game_returns_none_for_player_without_game() {
		new_test_ext().execute_with(|| {
			// Player 1 has no game
			let game = TicTacToe::get_player_game(&1);
			assert!(game.is_none());
		});
	}

	#[test]
	fn test_get_player_game_returns_player_game() {
		new_test_ext().execute_with(|| {
			// Create game between player 1 and 2 via matchmaking
			assert_ok!(TicTacToe::play_game(RuntimeOrigin::signed(1)));
			assert_ok!(TicTacToe::play_game(RuntimeOrigin::signed(2)));

			// Player 1 should have the game
			let result = TicTacToe::get_player_game(&1);
			assert!(result.is_some());
			let (game_id, game) = result.unwrap();
			assert_eq!(game_id, 0);
			assert_eq!(game.player_x, 1);
			assert_eq!(game.player_o, 2);
			assert_eq!(game.x_turn, true);
			assert_eq!(game.state, pallet::GameState::InProgress);

			// Player 2 should have the same game
			let result = TicTacToe::get_player_game(&2);
			assert!(result.is_some());
			let (game_id, _game) = result.unwrap();
			assert_eq!(game_id, 0);
		});
	}

	#[test]
	fn test_get_player_game_returns_finished_game() {
		new_test_ext().execute_with(|| {
			// Create a game via matchmaking
			assert_ok!(TicTacToe::play_game(RuntimeOrigin::signed(1)));
			assert_ok!(TicTacToe::play_game(RuntimeOrigin::signed(2)));

			// Player 1 should have a game in progress
			let result = TicTacToe::get_player_game(&1);
			assert!(result.is_some());
			let (_, game) = result.unwrap();
			assert_eq!(game.state, GameState::InProgress);

			// Play the game to completion (X wins)
			assert_ok!(TicTacToe::make_move(RuntimeOrigin::signed(1), 0, 0)); // X
			assert_ok!(TicTacToe::make_move(RuntimeOrigin::signed(2), 0, 3)); // O
			assert_ok!(TicTacToe::make_move(RuntimeOrigin::signed(1), 0, 1)); // X
			assert_ok!(TicTacToe::make_move(RuntimeOrigin::signed(2), 0, 4)); // O
			assert_ok!(TicTacToe::make_move(RuntimeOrigin::signed(1), 0, 2)); // X wins

			// Now both players should still have the game, but it's finished
			let result = TicTacToe::get_player_game(&1);
			assert!(result.is_some());
			let (_, game) = result.unwrap();
			assert_eq!(game.state, GameState::XWon);

			let result = TicTacToe::get_player_game(&2);
			assert!(result.is_some());
			let (_, game) = result.unwrap();
			assert_eq!(game.state, GameState::XWon);

			// Players can still join new games since current game is not InProgress
			assert!(!TicTacToe::has_active_game(&1));
			assert!(!TicTacToe::has_active_game(&2));
		});
	}

	#[test]
	fn test_cannot_join_matchmaking_with_active_game() {
		new_test_ext().execute_with(|| {
			// Create a game for player 1 and 2
			assert_ok!(TicTacToe::play_game(RuntimeOrigin::signed(1)));
			assert_ok!(TicTacToe::play_game(RuntimeOrigin::signed(2)));

			// Try to join matchmaking again with player 1 - should fail
			assert_eq!(
				TicTacToe::play_game(RuntimeOrigin::signed(1)),
				Err(Error::<Test>::AlreadyInGame.into())
			);

			// Try to join matchmaking with player 2 (who is also in a game) - should fail
			assert_eq!(
				TicTacToe::play_game(RuntimeOrigin::signed(2)),
				Err(Error::<Test>::AlreadyInGame.into())
			);

			// Player 1 should still have their game
			let game = TicTacToe::get_player_game(&1);
			assert!(game.is_some());
		});
	}

	#[test]
	fn test_runtime_api_game_conversion() {
		new_test_ext().execute_with(|| {
			// Create a game via matchmaking
			assert_ok!(TicTacToe::play_game(RuntimeOrigin::signed(1)));
			assert_ok!(TicTacToe::play_game(RuntimeOrigin::signed(2)));

			// Get the game via the API
			let result = TicTacToe::get_player_game(&1);
			assert!(result.is_some());
			let (game_id, pallet_game) = result.unwrap();
			assert_eq!(game_id, 0);

			// Convert to API game
			let api_game = Game::from_pallet_game(pallet_game.clone());

			// Verify all fields match
			assert_eq!(api_game.player_x, pallet_game.player_x);
			assert_eq!(api_game.player_o, pallet_game.player_o);
			assert_eq!(api_game.x_turn, pallet_game.x_turn);
			assert_eq!(api_game.board, pallet_game.board);
			assert_eq!(api_game.state, pallet_game.state);
			assert_eq!(api_game.last_move_block, pallet_game.last_move_block);
		});
	}

	#[test]
	fn test_get_player_game_after_matchmaking() {
		new_test_ext().execute_with(|| {
			// Player 1 joins queue
			assert_ok!(TicTacToe::play_game(RuntimeOrigin::signed(1)));

			// Player 1 should have no game (still in queue)
			let game = TicTacToe::get_player_game(&1);
			assert!(game.is_none());

			// Player 2 joins queue (creates game)
			assert_ok!(TicTacToe::play_game(RuntimeOrigin::signed(2)));

			// Now both players should have a game
			let game = TicTacToe::get_player_game(&1);
			assert!(game.is_some());

			let game = TicTacToe::get_player_game(&2);
			assert!(game.is_some());
		});
	}

	#[test]
	fn test_timeout_claim_before_elapsed_fails() {
		new_test_ext().execute_with(|| {
			// Set initial block number
			System::set_block_number(1);

			// Create a game
			assert_ok!(TicTacToe::play_game(RuntimeOrigin::signed(1)));
			assert_ok!(TicTacToe::play_game(RuntimeOrigin::signed(2)));

			// Move to block 5 (not enough time elapsed)
			System::set_block_number(5);

			// Player 2 tries to claim timeout (it's player 1's turn)
			assert_err!(
				TicTacToe::claim_timeout(RuntimeOrigin::signed(2), 0),
				Error::<Test>::TimeoutNotElapsed
			);
		});
	}

	#[test]
	fn test_timeout_claim_after_elapsed_succeeds() {
		new_test_ext().execute_with(|| {
			// Set initial block number
			System::set_block_number(1);

			// Create a game
			assert_ok!(TicTacToe::play_game(RuntimeOrigin::signed(1)));
			assert_ok!(TicTacToe::play_game(RuntimeOrigin::signed(2)));

			// Move to block 12 (more than 10 blocks elapsed)
			System::set_block_number(12);

			// Player 2 claims timeout (it's player 1's turn)
			assert_ok!(TicTacToe::claim_timeout(RuntimeOrigin::signed(2), 0));

			// Game should still exist but be finished
			let game = TicTacToe::games(0).unwrap();
			assert_eq!(game.state, GameState::OWon); // Player 2 (O) won by timeout

			// Players should not have an active game anymore
			assert!(!TicTacToe::has_active_game(&1));
			assert!(!TicTacToe::has_active_game(&2));
		});
	}

	#[test]
	fn test_timeout_claim_fails_if_its_callers_turn() {
		new_test_ext().execute_with(|| {
			// Set initial block number
			System::set_block_number(1);

			// Create a game
			assert_ok!(TicTacToe::play_game(RuntimeOrigin::signed(1)));
			assert_ok!(TicTacToe::play_game(RuntimeOrigin::signed(2)));

			// Move to block 12 (more than 10 blocks elapsed)
			System::set_block_number(12);

			// Player 1 tries to claim timeout, but it's their turn!
			assert_err!(
				TicTacToe::claim_timeout(RuntimeOrigin::signed(1), 0),
				Error::<Test>::NotYourTurn
			);
		});
	}

	#[test]
	fn test_move_updates_block_number() {
		new_test_ext().execute_with(|| {
			// Set initial block number
			System::set_block_number(1);

			// Create a game
			assert_ok!(TicTacToe::play_game(RuntimeOrigin::signed(1)));
			assert_ok!(TicTacToe::play_game(RuntimeOrigin::signed(2)));

			// Check initial block number
			let game = TicTacToe::games(0).unwrap();
			assert_eq!(game.last_move_block, 1);

			// Move to block 5 and make a move
			System::set_block_number(5);
			assert_ok!(TicTacToe::make_move(RuntimeOrigin::signed(1), 0, 0));

			// Check updated block number
			let game = TicTacToe::games(0).unwrap();
			assert_eq!(game.last_move_block, 5);
		});
	}

	#[test]
	fn test_player_games_storage_tracking() {
		new_test_ext().execute_with(|| {
			// Initially no players have games
			assert!(!TicTacToe::has_active_game(&1));
			assert!(!TicTacToe::has_active_game(&2));
			assert_eq!(TicTacToe::player_games(1), None);
			assert_eq!(TicTacToe::player_games(2), None);

			// Create a game
			assert_ok!(TicTacToe::play_game(RuntimeOrigin::signed(1)));
			assert_ok!(TicTacToe::play_game(RuntimeOrigin::signed(2)));

			// Both players should now have an active game
			assert!(TicTacToe::has_active_game(&1));
			assert!(TicTacToe::has_active_game(&2));
			assert_eq!(TicTacToe::player_games(1), Some(0));
			assert_eq!(TicTacToe::player_games(2), Some(0));

			// Make a winning move for player 1
			assert_ok!(TicTacToe::make_move(RuntimeOrigin::signed(1), 0, 0)); // X
			assert_ok!(TicTacToe::make_move(RuntimeOrigin::signed(2), 0, 3)); // O
			assert_ok!(TicTacToe::make_move(RuntimeOrigin::signed(1), 0, 1)); // X
			assert_ok!(TicTacToe::make_move(RuntimeOrigin::signed(2), 0, 4)); // O
			assert_ok!(TicTacToe::make_move(RuntimeOrigin::signed(1), 0, 2)); // X wins (0,1,2)

			// After game ends, PlayerGames entries remain but game is no longer active
			assert!(!TicTacToe::has_active_game(&1));
			assert!(!TicTacToe::has_active_game(&2));
			assert_eq!(TicTacToe::player_games(1), Some(0)); // Still points to game 0
			assert_eq!(TicTacToe::player_games(2), Some(0)); // Still points to game 0

			// Verify game is finished
			let game = TicTacToe::games(0).unwrap();
			assert_eq!(game.state, GameState::XWon);

			// Players should be able to join a new game now
			assert_ok!(TicTacToe::play_game(RuntimeOrigin::signed(1)));
			assert_ok!(TicTacToe::play_game(RuntimeOrigin::signed(3)));

			// Now player 1 should have a new active game (game_id 1)
			assert!(TicTacToe::has_active_game(&1));
			assert_eq!(TicTacToe::player_games(1), Some(1)); // Now points to new game
			assert!(TicTacToe::has_active_game(&3));
			assert_eq!(TicTacToe::player_games(3), Some(1));
		});
	}
}
