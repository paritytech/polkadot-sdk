// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! # Tic-Tac-Toe Pallet
//!
//! A pallet that implements a two-player tic-tac-toe game.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::pallet_prelude::{IsType, *};
use frame_system::pallet_prelude::*;
use scale_info::TypeInfo;
use sp_runtime::ArithmeticError;

extern crate alloc;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
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
	}

	/// Storage for active games, indexed by game ID
	#[pallet::storage]
	#[pallet::getter(fn games)]
	pub type Games<T: Config> = StorageMap<_, Blake2_128Concat, u32, Game<T>>;

	/// Next game ID
	#[pallet::storage]
	#[pallet::getter(fn next_game_id)]
	pub type NextGameId<T: Config> = StorageValue<_, u32, ValueQuery>;

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
	}

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
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create a new game
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().reads_writes(1, 2))]
		pub fn create_game(origin: OriginFor<T>, opponent: T::AccountId) -> DispatchResult {
			let who = ensure_signed(origin)?;

			log::info!(
				target: "runtime::tic-tac-toe",
				"Creating new game - Player X: {:?}, Player O: {:?}",
				who,
				opponent
			);

			// Ensure player is not playing against themselves
			ensure!(who != opponent, Error::<T>::CannotPlayAgainstSelf);

			let game_id = NextGameId::<T>::get();
			let next_id = game_id.checked_add(1).ok_or(ArithmeticError::Overflow)?;

			log::debug!(
				target: "runtime::tic-tac-toe",
				"Assigned game ID: {}, next ID will be: {}",
				game_id,
				next_id
			);

			let game = Game {
				player_x: who.clone(),
				player_o: opponent.clone(),
				x_turn: true,
				board: [Cell::Empty; 9],
				state: GameState::InProgress,
			};

			Games::<T>::insert(game_id, game);
			NextGameId::<T>::put(next_id);

			log::info!(
				target: "runtime::tic-tac-toe",
				"Game {} created successfully",
				game_id
			);

			Self::deposit_event(Event::GameCreated { game_id, player_x: who, player_o: opponent });

			Ok(())
		}

		/// Make a move
		#[pallet::call_index(1)]
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

				log::debug!(
					target: "runtime::tic-tac-toe",
					"Placed {:?} at position {} in game {}",
					piece,
					position,
					game_id
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
	}

	impl<T: Config> Pallet<T> {
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
	}
}

pub use pallet::*;
