//! # Battleship Pallet
//!
//! A pallet for playing battleship games on-chain with cryptographic commitments.
//!
//! ## Overview
//!
//! Players commit to their grid layout using merkle roots, then take turns attacking.
//! Each cell reveal is verified against the committed merkle root.
//! Standard ships: Carrier(5), Battleship(4), Cruiser(3), Submarine(3), Destroyer(2) = 17 cells.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

pub mod weights;
pub use weights::*;

use alloc::{vec, vec::Vec};
use bitvec::prelude::*;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{
	pallet_prelude::*,
	traits::{
		fungible::{Inspect, Mutate, MutateHold},
		tokens::{Precision, Preservation},
	},
};
use frame_system::pallet_prelude::*;
use scale_info::TypeInfo;
use sp_core::H256;
use sp_runtime::traits::{BlakeTwo256, Saturating};

/// Grid coordinate (0-9 for both x and y)
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Clone,
	Copy,
	TypeInfo,
	MaxEncodedLen,
	PartialEq,
	Eq,
	Debug,
)]
pub struct Coordinate {
	pub x: u8,
	pub y: u8,
}

impl Coordinate {
	/// Convert to cell index (0-99)
	pub fn to_index(&self) -> u32 {
		(self.y as u32) * 10 + (self.x as u32)
	}

	pub fn is_valid(&self) -> bool {
		self.x < 10 && self.y < 10
	}

	/// Check if two coordinates are adjacent (horizontally or vertically)
	pub fn is_adjacent(&self, other: &Coordinate) -> bool {
		let dx = (self.x as i16 - other.x as i16).abs();
		let dy = (self.y as i16 - other.y as i16).abs();
		(dx == 1 && dy == 0) || (dx == 0 && dy == 1)
	}
}

/// A single cell in the battleship grid
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, MaxEncodedLen, PartialEq, Eq, Debug,
)]
pub struct Cell {
	/// Random salt to prevent brute-forcing
	pub salt: [u8; 32],
	/// Whether the cell contains a ship
	pub is_occupied: bool,
}

impl Cell {
	/// Return the leaf representation for merkle tree (33 bytes)
	pub fn to_leaf(&self) -> [u8; 33] {
		let mut leaf = [0u8; 33];
		leaf[..32].copy_from_slice(&self.salt);
		leaf[32] = if self.is_occupied { 1 } else { 0 };
		leaf
	}
}

/// Bit array tracking revealed cells (100 bits)
#[derive(Encode, Decode, Clone, TypeInfo, PartialEq, Eq, Debug)]
pub struct RevealedCells(BitVec<u8, Lsb0>);

impl Default for RevealedCells {
	fn default() -> Self {
		Self(bitvec![u8, Lsb0; 0; 100])
	}
}

impl MaxEncodedLen for RevealedCells {
	fn max_encoded_len() -> usize {
		// 100 bits = 13 bytes + compact length prefix
		1 + 13
	}
}

impl RevealedCells {
	pub fn get(&self, index: u32) -> bool {
		self.0.get(index as usize).as_deref().copied().unwrap_or(false)
	}

	pub fn set(&mut self, index: u32) {
		self.0.set(index as usize, true);
	}

	pub fn count_ones(&self) -> u32 {
		self.0.count_ones() as u32
	}
}

/// Cell reveal with merkle proof
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, PartialEq, Eq, Debug)]
pub struct CellReveal {
	/// The cell being revealed
	pub cell: Cell,
	/// Merkle proof nodes (max 7 for 100-leaf tree)
	pub proof: BoundedVec<H256, ConstU32<8>>,
}

/// Player role in a game
#[derive(Encode, Decode, Clone, Copy, TypeInfo, MaxEncodedLen, PartialEq, Eq, Debug)]
pub enum PlayerRole {
	Player1,
	Player2,
}

/// Reason for game ending
#[derive(
	Encode, Decode, DecodeWithMemTracking, Clone, TypeInfo, MaxEncodedLen, PartialEq, Eq, Debug,
)]
pub enum GameEndReason {
	/// Winner revealed valid grid
	ValidWin,
	/// Opponent timed out
	Timeout,
	/// Opponent surrendered
	Surrender,
	/// Invalid merkle proof during reveal (cheating)
	Cheating,
	/// Winner's grid had invalid ship placement
	InvalidWinnerGrid,
}

/// Game phases
#[derive(Encode, Decode, Clone, TypeInfo, MaxEncodedLen, PartialEq, Eq, Debug)]
pub enum GamePhase {
	/// Waiting for second player to join
	WaitingForOpponent,
	/// Both players are setting up their grids
	Setup { player1_ready: bool, player2_ready: bool },
	/// Active gameplay
	Playing {
		/// Who's turn it is
		current_turn: PlayerRole,
		/// The last attack coordinate (defender must respond)
		pending_attack: Option<Coordinate>,
	},
	/// Winner must reveal full grid for validation
	PendingWinnerReveal { winner: PlayerRole },
	/// Game has ended
	Finished { winner: PlayerRole, reason: GameEndReason },
}

pub type GameId = u64;

/// Per-player game data (stored separately to avoid loading full state)
#[derive(Encode, Decode, Clone, TypeInfo, PartialEq, Eq, Debug)]
pub struct PlayerData {
	/// Committed grid merkle root
	pub grid_root: Option<H256>,
	/// Cells revealed on this player's grid
	pub revealed: RevealedCells,
	/// Hit coordinates on this player's grid (for ship validation)
	pub hit_cells: BoundedVec<Coordinate, ConstU32<17>>,
	/// Hits scored by this player on opponent's grid
	pub hits: u8,
}

impl Default for PlayerData {
	fn default() -> Self {
		Self {
			grid_root: None,
			revealed: RevealedCells::default(),
			hit_cells: BoundedVec::new(),
			hits: 0,
		}
	}
}

impl MaxEncodedLen for PlayerData {
	fn max_encoded_len() -> usize {
		// Option<H256> + RevealedCells + BoundedVec<Coordinate, 17> + u8
		1 + 32 + RevealedCells::max_encoded_len() + 1 + 17 * Coordinate::max_encoded_len() + 1
	}
}

/// Core game state (lightweight, loaded on every operation)
#[derive(Encode, Decode, Clone, TypeInfo, MaxEncodedLen, PartialEq, Eq, Debug)]
#[scale_info(skip_type_params(T))]
pub struct Game<T: Config> {
	/// Unique game ID
	pub id: GameId,
	/// Player 1 account
	pub player1: T::AccountId,
	/// Player 2 account (None during WaitingForOpponent)
	pub player2: Option<T::AccountId>,
	/// Pot amount per player
	pub pot_amount: BalanceOf<T>,
	/// Current game phase
	pub phase: GamePhase,
	/// Block when last action was taken (for timeout)
	pub last_action_block: BlockNumberFor<T>,
}

type BalanceOf<T> =
	<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// Currency for pot management
		type Currency: Inspect<Self::AccountId>
			+ Mutate<Self::AccountId>
			+ MutateHold<Self::AccountId, Reason = Self::RuntimeHoldReason>;

		/// Hold reason
		type RuntimeHoldReason: From<HoldReason>;

		/// Timeout in blocks for individual turns
		#[pallet::constant]
		type TurnTimeout: Get<BlockNumberFor<Self>>;

		/// Timeout in blocks after which abandoned games are aborted and funds burned
		#[pallet::constant]
		type AbandonTimeout: Get<BlockNumberFor<Self>>;

		/// Weight info
		type WeightInfo: WeightInfo;
	}

	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Funds held for battleship game pot
		GamePot,
	}

	#[pallet::storage]
	pub type NextGameId<T> = StorageValue<_, GameId, ValueQuery>;

	#[pallet::storage]
	pub type Games<T: Config> = StorageMap<_, Blake2_128Concat, GameId, Game<T>>;

	#[pallet::storage]
	pub type PlayerDataStorage<T: Config> =
		StorageDoubleMap<_, Blake2_128Concat, GameId, Blake2_128Concat, T::AccountId, PlayerData>;

	#[pallet::storage]
	pub type PlayerGame<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, GameId>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new game was created
		GameCreated { game_id: GameId, player1: T::AccountId, pot_amount: BalanceOf<T> },
		/// A player joined an existing game
		GameJoined { game_id: GameId, player2: T::AccountId },
		/// A player committed their grid
		GridCommitted { game_id: GameId, player: T::AccountId },
		/// Game started (both grids committed)
		GameStarted { game_id: GameId },
		/// An attack was made
		AttackMade { game_id: GameId, attacker: T::AccountId, coordinate: Coordinate },
		/// Attack result revealed
		AttackRevealed { game_id: GameId, coordinate: Coordinate, hit: bool },
		/// All ships sunk, pending winner grid reveal
		AllShipsSunk { game_id: GameId, pending_winner: T::AccountId },
		/// Game ended
		GameEnded {
			game_id: GameId,
			winner: T::AccountId,
			loser: T::AccountId,
			reason: GameEndReason,
			prize: BalanceOf<T>,
		},
		/// Game abandoned due to inactivity - funds burned
		GameAbandoned {
			game_id: GameId,
			burned_amount: BalanceOf<T>,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Game does not exist
		GameNotFound,
		/// Player is already in an active game
		PlayerAlreadyInGame,
		/// Not your turn
		NotYourTurn,
		/// Invalid game phase for this action
		InvalidGamePhase,
		/// Invalid coordinate (out of bounds)
		InvalidCoordinate,
		/// Merkle proof verification failed
		InvalidMerkleProof,
		/// Cell was already revealed
		CellAlreadyRevealed,
		/// Not a participant of this game
		NotGameParticipant,
		/// Grid already committed
		GridAlreadyCommitted,
		/// Timeout has not been reached yet
		TimeoutNotReached,
		/// Cannot claim timeout
		CannotClaimTimeout,
		/// Invalid grid size
		InvalidGridSize,
		/// Invalid ship placement
		InvalidShipPlacement,
		/// Cannot join own game
		CannotJoinOwnGame,
		/// Game ID counter overflow
		GameIdOverflow,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_idle(_n: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			Self::cleanup_abandoned_games(remaining_weight)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create a new game with a pot amount
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::create_game())]
		pub fn create_game(origin: OriginFor<T>, pot_amount: BalanceOf<T>) -> DispatchResult {
			let player1 = ensure_signed(origin)?;

			ensure!(!PlayerGame::<T>::contains_key(&player1), Error::<T>::PlayerAlreadyInGame);

			// Hold the pot amount
			T::Currency::hold(&HoldReason::GamePot.into(), &player1, pot_amount)?;

			let game_id = NextGameId::<T>::try_mutate(|id| -> Result<GameId, Error<T>> {
				let current = *id;
				*id = id.checked_add(1).ok_or(Error::<T>::GameIdOverflow)?;
				Ok(current)
			})?;

			let game = Game {
				id: game_id,
				player1: player1.clone(),
				player2: None,
				pot_amount,
				phase: GamePhase::WaitingForOpponent,
				last_action_block: frame_system::Pallet::<T>::block_number(),
			};

			Games::<T>::insert(game_id, game);
			PlayerDataStorage::<T>::insert(game_id, &player1, PlayerData::default());
			PlayerGame::<T>::insert(&player1, game_id);

			Self::deposit_event(Event::GameCreated { game_id, player1, pot_amount });

			Ok(())
		}

		/// Join an existing game
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::join_game())]
		pub fn join_game(origin: OriginFor<T>, game_id: GameId) -> DispatchResult {
			let player2 = ensure_signed(origin)?;

			ensure!(!PlayerGame::<T>::contains_key(&player2), Error::<T>::PlayerAlreadyInGame);

			Games::<T>::try_mutate(game_id, |maybe_game| -> DispatchResult {
				let game = maybe_game.as_mut().ok_or(Error::<T>::GameNotFound)?;

				ensure!(game.phase == GamePhase::WaitingForOpponent, Error::<T>::InvalidGamePhase);
				ensure!(game.player1 != player2, Error::<T>::CannotJoinOwnGame);

				// Hold pot from player2
				T::Currency::hold(&HoldReason::GamePot.into(), &player2, game.pot_amount)?;

				game.player2 = Some(player2.clone());
				game.phase = GamePhase::Setup { player1_ready: false, player2_ready: false };
				game.last_action_block = frame_system::Pallet::<T>::block_number();

				PlayerDataStorage::<T>::insert(game_id, &player2, PlayerData::default());
				PlayerGame::<T>::insert(&player2, game_id);

				Self::deposit_event(Event::GameJoined { game_id, player2 });

				Ok(())
			})
		}

		/// Commit grid merkle root
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::commit_grid())]
		pub fn commit_grid(
			origin: OriginFor<T>,
			game_id: GameId,
			grid_root: H256,
		) -> DispatchResult {
			let player = ensure_signed(origin)?;

			// Update player data
			PlayerDataStorage::<T>::try_mutate(game_id, &player, |maybe_data| -> DispatchResult {
				let data = maybe_data.as_mut().ok_or(Error::<T>::NotGameParticipant)?;
				ensure!(data.grid_root.is_none(), Error::<T>::GridAlreadyCommitted);
				data.grid_root = Some(grid_root);
				Ok(())
			})?;

			// Update game state
			Games::<T>::try_mutate(game_id, |maybe_game| -> DispatchResult {
				let game = maybe_game.as_mut().ok_or(Error::<T>::GameNotFound)?;

				let GamePhase::Setup { ref mut player1_ready, ref mut player2_ready } = game.phase
				else {
					return Err(Error::<T>::InvalidGamePhase.into());
				};

				let is_player1 = game.player1 == player;
				if is_player1 {
					*player1_ready = true;
				} else {
					*player2_ready = true;
				}

				game.last_action_block = frame_system::Pallet::<T>::block_number();

				Self::deposit_event(Event::GridCommitted { game_id, player: player.clone() });

				// Check if both players are ready
				if *player1_ready && *player2_ready {
					game.phase = GamePhase::Playing {
						current_turn: PlayerRole::Player1,
						pending_attack: None,
					};
					Self::deposit_event(Event::GameStarted { game_id });
				}

				Ok(())
			})
		}

		/// Attack a coordinate
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::attack())]
		pub fn attack(
			origin: OriginFor<T>,
			game_id: GameId,
			coordinate: Coordinate,
		) -> DispatchResult {
			let attacker = ensure_signed(origin)?;

			ensure!(coordinate.is_valid(), Error::<T>::InvalidCoordinate);

			Games::<T>::try_mutate(game_id, |maybe_game| -> DispatchResult {
				let game = maybe_game.as_mut().ok_or(Error::<T>::GameNotFound)?;

				let GamePhase::Playing { current_turn, pending_attack } = &mut game.phase else {
					return Err(Error::<T>::InvalidGamePhase.into());
				};

				// Must not have a pending attack
				ensure!(pending_attack.is_none(), Error::<T>::NotYourTurn);

				// Check it's the attacker's turn
				let is_player1 = game.player1 == attacker;
				let is_player2 = game.player2.as_ref() == Some(&attacker);

				ensure!(is_player1 || is_player2, Error::<T>::NotGameParticipant);

				let expected_turn =
					if is_player1 { PlayerRole::Player1 } else { PlayerRole::Player2 };
				ensure!(*current_turn == expected_turn, Error::<T>::NotYourTurn);

				// Get opponent to check if cell was already revealed
				let opponent = if is_player1 {
					game.player2.as_ref().ok_or(Error::<T>::InvalidGamePhase)?
				} else {
					&game.player1
				};

				// Check cell hasn't been attacked before (read opponent's data)
				let opponent_data = PlayerDataStorage::<T>::get(game_id, opponent)
					.ok_or(Error::<T>::GameNotFound)?;
				ensure!(
					!opponent_data.revealed.get(coordinate.to_index()),
					Error::<T>::CellAlreadyRevealed
				);

				*pending_attack = Some(coordinate);
				game.last_action_block = frame_system::Pallet::<T>::block_number();

				Self::deposit_event(Event::AttackMade { game_id, attacker, coordinate });

				Ok(())
			})
		}

		/// Reveal a cell in response to an attack
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::reveal_cell())]
		pub fn reveal_cell(
			origin: OriginFor<T>,
			game_id: GameId,
			reveal: CellReveal,
		) -> DispatchResult {
			let defender = ensure_signed(origin)?;

			// First get the game state to determine roles
			let mut game = Games::<T>::get(game_id).ok_or(Error::<T>::GameNotFound)?;

			let (current_turn, attack_coord) = match &game.phase {
				GamePhase::Playing { current_turn, pending_attack: Some(coord) } =>
					(*current_turn, *coord),
				_ => return Err(Error::<T>::InvalidGamePhase.into()),
			};

			// Determine defender role
			let is_player1 = game.player1 == defender;
			let is_player2 = game.player2.as_ref() == Some(&defender);

			ensure!(is_player1 || is_player2, Error::<T>::NotGameParticipant);

			// Defender is the one NOT currently attacking
			let defender_role = match current_turn {
				PlayerRole::Player1 => PlayerRole::Player2,
				PlayerRole::Player2 => PlayerRole::Player1,
			};

			let is_defender = match defender_role {
				PlayerRole::Player1 => is_player1,
				PlayerRole::Player2 => is_player2,
			};
			ensure!(is_defender, Error::<T>::NotYourTurn);

			// Get and update defender's data
			let total_revealed =
				PlayerDataStorage::<T>::try_mutate(game_id, &defender, |maybe_data| {
					let data = maybe_data.as_mut().ok_or(Error::<T>::NotGameParticipant)?;

					// Get the committed grid root for the defender
					let grid_root = data.grid_root.ok_or(Error::<T>::InvalidGamePhase)?;

					// Verify merkle proof
					let leaf = reveal.cell.to_leaf();
					let valid = binary_merkle_tree::verify_proof::<BlakeTwo256, _, _>(
						&grid_root,
						reveal.proof.iter().cloned(),
						100,
						attack_coord.to_index(),
						&leaf,
					);

					if !valid {
						// Cheating detected - return special marker
						return Err(Error::<T>::InvalidMerkleProof);
					}

					// Mark cell as revealed
					data.revealed.set(attack_coord.to_index());

					let is_hit = reveal.cell.is_occupied;

					if is_hit {
						// Add to hit cells for validation
						data.hit_cells
							.try_push(attack_coord)
							.map_err(|_| Error::<T>::InvalidShipPlacement)?;

						// Validate hit pattern
						if !Self::validate_hit_pattern(&data.hit_cells) {
							return Err(Error::<T>::InvalidShipPlacement);
						}
					}

					Ok((data.revealed.count_ones(), is_hit))
				});

			// Handle errors from defender data update
			let (total_revealed, is_hit) = match total_revealed {
				Ok(result) => result,
				Err(Error::<T>::InvalidMerkleProof) | Err(Error::<T>::InvalidShipPlacement) => {
					// Cheating detected - defender loses
					let (winner, loser) = match current_turn {
						PlayerRole::Player1 =>
							(game.player1.clone(), game.player2.clone().unwrap()),
						PlayerRole::Player2 =>
							(game.player2.clone().unwrap(), game.player1.clone()),
					};
					return Self::finalize_game(&mut game, game_id, winner, loser, GameEndReason::Cheating);
				},
				Err(e) => return Err(e.into()),
			};

			// Update attacker's hit counter in PlayerData
			let attacker = match current_turn {
				PlayerRole::Player1 => &game.player1,
				PlayerRole::Player2 => game.player2.as_ref().unwrap(),
			};

			let total_hits = if is_hit {
				PlayerDataStorage::<T>::mutate(game_id, attacker, |maybe_data| {
					if let Some(data) = maybe_data {
						data.hits += 1;
						data.hits
					} else {
						0
					}
				})
			} else {
				PlayerDataStorage::<T>::get(game_id, attacker)
					.map(|d| d.hits)
					.unwrap_or(0)
			};

			Self::deposit_event(Event::AttackRevealed {
				game_id,
				coordinate: attack_coord,
				hit: is_hit,
			});

			if total_hits >= 17 {
				// Winner must reveal their grid
				let pending_winner = match current_turn {
					PlayerRole::Player1 => game.player1.clone(),
					PlayerRole::Player2 => game.player2.clone().unwrap(),
				};
				game.phase = GamePhase::PendingWinnerReveal { winner: current_turn };
				game.last_action_block = frame_system::Pallet::<T>::block_number();
				Self::deposit_event(Event::AllShipsSunk { game_id, pending_winner });
			} else if total_revealed >= 100 {
				// All cells revealed but < 17 hits: defender had invalid grid
				let (winner, loser) = match current_turn {
					PlayerRole::Player1 => (game.player1.clone(), game.player2.clone().unwrap()),
					PlayerRole::Player2 => (game.player2.clone().unwrap(), game.player1.clone()),
				};
				return Self::finalize_game(&mut game, game_id, winner, loser, GameEndReason::Cheating);
			} else {
				// Switch turns
				game.phase = GamePhase::Playing {
					current_turn: match current_turn {
						PlayerRole::Player1 => PlayerRole::Player2,
						PlayerRole::Player2 => PlayerRole::Player1,
					},
					pending_attack: None,
				};
				game.last_action_block = frame_system::Pallet::<T>::block_number();
			}

			// Save updated game state
			Games::<T>::insert(game_id, game);

			Ok(())
		}

		/// Reveal winner's full grid for validation
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::reveal_winner_grid())]
		pub fn reveal_winner_grid(
			origin: OriginFor<T>,
			game_id: GameId,
			full_grid: Vec<Cell>,
		) -> DispatchResult {
			let caller = ensure_signed(origin)?;

			let mut game = Games::<T>::get(game_id).ok_or(Error::<T>::GameNotFound)?;

			let winner_role = match &game.phase {
				GamePhase::PendingWinnerReveal { winner } => *winner,
				_ => return Err(Error::<T>::InvalidGamePhase.into()),
			};

			// Verify caller is the winner
			let (winner, loser) = match winner_role {
				PlayerRole::Player1 => {
					ensure!(game.player1 == caller, Error::<T>::NotGameParticipant);
					(game.player1.clone(), game.player2.clone().unwrap())
				},
				PlayerRole::Player2 => {
					ensure!(game.player2.as_ref() == Some(&caller), Error::<T>::NotGameParticipant);
					(game.player2.clone().unwrap(), game.player1.clone())
				},
			};

			// Get winner's grid root from their player data
			let winner_data =
				PlayerDataStorage::<T>::get(game_id, &winner).ok_or(Error::<T>::GameNotFound)?;
			let winner_grid_root = winner_data.grid_root.ok_or(Error::<T>::InvalidGamePhase)?;

			// Verify grid size
			ensure!(full_grid.len() == 100, Error::<T>::InvalidGridSize);

			// Compute merkle root and verify
			let leaves: Vec<[u8; 33]> = full_grid.iter().map(|c| c.to_leaf()).collect();
			let computed_root = binary_merkle_tree::merkle_root::<BlakeTwo256, _>(leaves);

			if computed_root != winner_grid_root {
				// Grid doesn't match commitment - winner was cheating
				return Self::finalize_game(
					&mut game,
					game_id,
					loser,
					winner,
					GameEndReason::InvalidWinnerGrid,
				);
			}

			// Validate ship placement
			if !Self::validate_ship_placement(&full_grid) {
				// Invalid ship placement - winner was cheating
				return Self::finalize_game(
					&mut game,
					game_id,
					loser,
					winner,
					GameEndReason::InvalidWinnerGrid,
				);
			}

			// Valid win!
			Self::finalize_game(&mut game, game_id, winner, loser, GameEndReason::ValidWin)
		}

		/// Claim win by timeout
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::claim_timeout_win())]
		pub fn claim_timeout_win(origin: OriginFor<T>, game_id: GameId) -> DispatchResult {
			let claimer = ensure_signed(origin)?;

			let mut game = Games::<T>::get(game_id).ok_or(Error::<T>::GameNotFound)?;

			// Verify claimer is a participant
			let is_player1 = game.player1 == claimer;
			let is_player2 = game.player2.as_ref() == Some(&claimer);
			ensure!(is_player1 || is_player2, Error::<T>::NotGameParticipant);

			let current_block = frame_system::Pallet::<T>::block_number();
			let timeout_block = game.last_action_block.saturating_add(T::TurnTimeout::get());
			ensure!(current_block >= timeout_block, Error::<T>::TimeoutNotReached);

			let (winner, loser) = match &game.phase {
				GamePhase::WaitingForOpponent => {
					// Cancel game, return funds to player1
					T::Currency::release(
						&HoldReason::GamePot.into(),
						&game.player1,
						game.pot_amount,
						Precision::Exact,
					)?;
					Games::<T>::remove(game_id);
					PlayerDataStorage::<T>::remove(game_id, &game.player1);
					PlayerGame::<T>::remove(&game.player1);
					return Ok(());
				},
				GamePhase::Setup { player1_ready, player2_ready } => {
					if !player1_ready && *player2_ready {
						(game.player2.clone().unwrap(), game.player1.clone())
					} else if *player1_ready && !player2_ready {
						(game.player1.clone(), game.player2.clone().unwrap())
					} else {
						return Err(Error::<T>::CannotClaimTimeout.into());
					}
				},
				GamePhase::Playing { current_turn, pending_attack } => {
					if pending_attack.is_some() {
						// Defender timed out
						match current_turn {
							PlayerRole::Player1 =>
								(game.player1.clone(), game.player2.clone().unwrap()),
							PlayerRole::Player2 =>
								(game.player2.clone().unwrap(), game.player1.clone()),
						}
					} else {
						// Current player (attacker) timed out
						match current_turn {
							PlayerRole::Player1 =>
								(game.player2.clone().unwrap(), game.player1.clone()),
							PlayerRole::Player2 =>
								(game.player1.clone(), game.player2.clone().unwrap()),
						}
					}
				},
				GamePhase::PendingWinnerReveal { winner } => {
					// Winner hasn't revealed - loser wins
					match winner {
						PlayerRole::Player1 =>
							(game.player2.clone().unwrap(), game.player1.clone()),
						PlayerRole::Player2 =>
							(game.player1.clone(), game.player2.clone().unwrap()),
					}
				},
				GamePhase::Finished { .. } => {
					return Err(Error::<T>::InvalidGamePhase.into());
				},
			};

			// Claimer must be the winner
			ensure!(winner == claimer, Error::<T>::CannotClaimTimeout);

			Self::finalize_game(&mut game, game_id, winner, loser, GameEndReason::Timeout)
		}

		/// Surrender the game
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::surrender())]
		pub fn surrender(origin: OriginFor<T>, game_id: GameId) -> DispatchResult {
			let player = ensure_signed(origin)?;

			Games::<T>::try_mutate(game_id, |maybe_game| -> DispatchResult {
				let game = maybe_game.as_mut().ok_or(Error::<T>::GameNotFound)?;

				let is_player1 = game.player1 == player;
				let is_player2 = game.player2.as_ref() == Some(&player);
				ensure!(is_player1 || is_player2, Error::<T>::NotGameParticipant);

				// Can't surrender if game hasn't started or already finished
				match &game.phase {
					GamePhase::WaitingForOpponent | GamePhase::Finished { .. } => {
						return Err(Error::<T>::InvalidGamePhase.into());
					},
					_ => {},
				}

				let (winner, loser) = if is_player1 {
					(
						game.player2.clone().ok_or(Error::<T>::InvalidGamePhase)?,
						game.player1.clone(),
					)
				} else {
					(game.player1.clone(), player)
				};

				Self::finalize_game(game, game_id, winner, loser, GameEndReason::Surrender)
			})
		}
	}

	impl<T: Config> Pallet<T> {
		/// Finalize a game and pay out the winner
		fn finalize_game(
			game: &mut Game<T>,
			game_id: GameId,
			winner: T::AccountId,
			loser: T::AccountId,
			reason: GameEndReason,
		) -> DispatchResult {
			let pot_amount = game.pot_amount;
			let total_pot = pot_amount.saturating_add(pot_amount);

			// Release holds from both players
			T::Currency::release(
				&HoldReason::GamePot.into(),
				&game.player1,
				pot_amount,
				Precision::Exact,
			)?;

			if let Some(ref p2) = game.player2 {
				T::Currency::release(
					&HoldReason::GamePot.into(),
					p2,
					pot_amount,
					Precision::Exact,
				)?;
			}

			// Transfer loser's pot to winner
			T::Currency::transfer(&loser, &winner, pot_amount, Preservation::Expendable)?;

			// Clean up player mappings
			PlayerGame::<T>::remove(&game.player1);
			PlayerDataStorage::<T>::remove(game_id, &game.player1);
			if let Some(ref p2) = game.player2 {
				PlayerGame::<T>::remove(p2);
				PlayerDataStorage::<T>::remove(game_id, p2);
			}

			// Remove game from storage
			Games::<T>::remove(game_id);

			Self::deposit_event(Event::GameEnded {
				game_id,
				winner,
				loser,
				reason,
				prize: total_pot,
			});

			Ok(())
		}

		/// Validate that revealed hits can still form valid ships
		fn validate_hit_pattern(hits: &[Coordinate]) -> bool {
			if hits.is_empty() {
				return true;
			}

			let components = Self::find_connected_components(hits);

			// At most 5 ships
			if components.len() > 5 {
				return false;
			}

			let mut sizes = Vec::new();
			for component in &components {
				// Check straight line
				let all_same_x = component.iter().all(|c| c.x == component[0].x);
				let all_same_y = component.iter().all(|c| c.y == component[0].y);
				if !all_same_x && !all_same_y {
					return false;
				}

				// Check size <= 5
				if component.len() > 5 {
					return false;
				}
				sizes.push(component.len());
			}

			// Validate sizes are achievable
			sizes.sort();
			sizes.reverse();
			let mut remaining = vec![5usize, 4, 3, 3, 2];
			for size in sizes {
				if let Some(pos) = remaining.iter().position(|&s| s >= size) {
					remaining.remove(pos);
				} else {
					return false;
				}
			}
			true
		}

		/// Validate full ship placement
		fn validate_ship_placement(cells: &[Cell]) -> bool {
			if cells.len() != 100 {
				return false;
			}

			let occupied: Vec<u8> = cells
				.iter()
				.enumerate()
				.filter(|(_, c)| c.is_occupied)
				.map(|(i, _)| i as u8)
				.collect();

			if occupied.len() != 17 {
				return false;
			}

			// Convert to coordinates
			let coords: Vec<Coordinate> =
				occupied.iter().map(|&i| Coordinate { x: i % 10, y: i / 10 }).collect();

			let components = Self::find_connected_components(&coords);

			if components.len() != 5 {
				return false;
			}

			// Check each component is a straight line
			for component in &components {
				let all_same_x = component.iter().all(|c| c.x == component[0].x);
				let all_same_y = component.iter().all(|c| c.y == component[0].y);
				if !all_same_x && !all_same_y {
					return false;
				}
			}

			// Check sizes
			let mut sizes: Vec<usize> = components.iter().map(|c| c.len()).collect();
			sizes.sort();
			sizes == vec![2, 3, 3, 4, 5]
		}

		/// Find connected components of coordinates
		fn find_connected_components(coords: &[Coordinate]) -> Vec<Vec<Coordinate>> {
			let mut visited = vec![false; coords.len()];
			let mut components = Vec::new();

			for i in 0..coords.len() {
				if visited[i] {
					continue;
				}

				let mut component = Vec::new();
				let mut stack = vec![i];

				while let Some(idx) = stack.pop() {
					if visited[idx] {
						continue;
					}
					visited[idx] = true;
					component.push(coords[idx]);

					// Find adjacent unvisited coords
					for (j, coord) in coords.iter().enumerate() {
						if !visited[j] && coords[idx].is_adjacent(coord) {
							stack.push(j);
						}
					}
				}

				components.push(component);
			}

			components
		}

		/// Clean up abandoned games in on_idle
		fn cleanup_abandoned_games(remaining_weight: Weight) -> Weight {
			let current_block = frame_system::Pallet::<T>::block_number();
			let abandon_timeout = T::AbandonTimeout::get();

			// Base weight for the function
			let base_weight = Weight::from_parts(10_000, 0);
			let per_game_weight = Weight::from_parts(50_000_000, 0)
				.saturating_add(T::DbWeight::get().reads(1))
				.saturating_add(T::DbWeight::get().writes(4));

			if remaining_weight.ref_time() < base_weight.ref_time() {
				return Weight::zero();
			}

			let mut used_weight = base_weight;
			let mut games_to_abort = Vec::new();

			// Find abandoned games
			for (game_id, game) in Games::<T>::iter() {
				if used_weight.saturating_add(per_game_weight).ref_time() > remaining_weight.ref_time()
				{
					break;
				}
				used_weight = used_weight.saturating_add(T::DbWeight::get().reads(1));

				let timeout_block = game.last_action_block.saturating_add(abandon_timeout);
				if current_block >= timeout_block {
					// Game is abandoned
					games_to_abort.push((game_id, game));
				}
			}

			// Abort abandoned games
			for (game_id, game) in games_to_abort {
				if used_weight.saturating_add(per_game_weight).ref_time() > remaining_weight.ref_time()
				{
					break;
				}

				if Self::abort_abandoned_game(game_id, game).is_ok() {
					used_weight = used_weight.saturating_add(per_game_weight);
				}
			}

			used_weight
		}

		/// Abort an abandoned game and burn the funds
		fn abort_abandoned_game(game_id: GameId, game: Game<T>) -> DispatchResult {
			let pot_amount = game.pot_amount;
			let mut total_burned = BalanceOf::<T>::default();

			// Release and burn player1's pot
			T::Currency::release(
				&HoldReason::GamePot.into(),
				&game.player1,
				pot_amount,
				Precision::Exact,
			)?;
			// Burn by transferring to a non-existent account (or use Currency::burn if available)
			let burned1 = T::Currency::burn_from(
				&game.player1,
				pot_amount,
				Precision::BestEffort,
				frame_support::traits::tokens::Fortitude::Force,
			)
			.unwrap_or_default();
			total_burned = total_burned.saturating_add(burned1);

			// Release and burn player2's pot if they joined
			if let Some(ref p2) = game.player2 {
				T::Currency::release(&HoldReason::GamePot.into(), p2, pot_amount, Precision::Exact)?;
				let burned2 = T::Currency::burn_from(
					p2,
					pot_amount,
					Precision::BestEffort,
					frame_support::traits::tokens::Fortitude::Force,
				)
				.unwrap_or_default();
				total_burned = total_burned.saturating_add(burned2);
			}

			// Clean up player mappings
			PlayerGame::<T>::remove(&game.player1);
			PlayerDataStorage::<T>::remove(game_id, &game.player1);
			if let Some(ref p2) = game.player2 {
				PlayerGame::<T>::remove(p2);
				PlayerDataStorage::<T>::remove(game_id, p2);
			}

			// Remove game from storage
			Games::<T>::remove(game_id);

			Self::deposit_event(Event::GameAbandoned { game_id, burned_amount: total_burned });

			Ok(())
		}
	}
}
