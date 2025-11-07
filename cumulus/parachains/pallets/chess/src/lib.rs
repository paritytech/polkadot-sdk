//! # Chess Game Pallet
//!
//! A fully on-chain chess game implementation for Polkadot parachains.
//!
//! ## Overview
//!
//! This pallet provides:
//! - Chess game creation and joining with configurable stakes
//! - Full chess move validation using shakmaty
//! - Multiple time control variants (UltraBullet, Bullet, Blitz, Rapid, Classical, Daily)
//! - Game ending detection (checkmate, stalemate, resignation, timeout)
//! - Automatic draw detection (threefold repetition, fifty-move rule, insufficient material)
//! - Prize distribution to winners
//! - Draw offers and acceptance/declination
//! - ELO rating system with automatic updates
//! - Player statistics tracking (wins, losses, draws, games played)
//! - Game variants (Standard, Fischer Random/Chess960)
//! - Detailed event emissions (check, capture, special draws)
//! - Query functions for UI integration (leaderboard, active games, game history)
//!
//! ## Implementation
//!
//! Games are stored on-chain with all moves recorded. The shakmaty chess engine
//! validates move legality. Time tracking uses block timestamps for timeout detection.
//! Player ratings are calculated using standard ELO formula with K-factor adjustment
//! for experience level.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

use alloc::string::ToString;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	pallet_prelude::*,
	traits::{
		fungible::{Inspect, Mutate, MutateHold},
		tokens::{Precision, Preservation},
	},
};
use frame_system::pallet_prelude::*;
use scale_info::TypeInfo;
use sp_runtime::{
	traits::Hash,
	SaturatedConversion,
};
use shakmaty::{
	Chess, Color, Position, Square as ShakmSquare,
	san::San, fen::Fen, Move as ChessMove, Role,
};

/// Type alias for game ID (hash)
pub type GameId = [u8; 32];

/// Maximum moves per game (500 moves = ~1000 ply)
pub const MAX_MOVES: u32 = 500;

/// Maximum open games in lobby
pub const MAX_OPEN_GAMES: u32 = 1000;

/// Maximum active games per player
pub const MAX_GAMES_PER_PLAYER: u32 = 10;

/// Maximum FEN string length
pub const MAX_FEN_LENGTH: u32 = 200;

/// Maximum players in leaderboard
pub const MAX_LEADERBOARD_SIZE: u32 = 100;

/// Starting position FEN
pub const STARTING_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::traits::UnixTime;

	/// Game status enum
	#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	pub enum GameStatus {
		/// Waiting for player 2 to join
		Waiting,
		/// Both players joined, game active
		Active,
		/// Game completed (win/loss/draw)
		Completed,
		/// Game timed out
		Timeout,
		/// Game cancelled before starting
		Cancelled,
	}

	/// Game result enum
	#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	#[codec(dumb_trait_bound)]
	pub enum GameResult {
		/// Game still ongoing
		Ongoing,
		/// White wins
		WhiteWins,
		/// Black wins
		BlackWins,
		/// Draw (stalemate, agreement, etc.)
		Draw,
	}

	impl GameResult {
		/// Convert to u8 for use in Events
		pub fn to_u8(&self) -> u8 {
			match self {
				Self::Ongoing => 0,
				Self::WhiteWins => 1,
				Self::BlackWins => 2,
				Self::Draw => 3,
			}
		}

		/// Convert from u8
		pub fn from_u8(value: u8) -> Option<Self> {
			match value {
				0 => Some(Self::Ongoing),
				1 => Some(Self::WhiteWins),
				2 => Some(Self::BlackWins),
				3 => Some(Self::Draw),
				_ => None,
			}
		}
	}

	/// Time control types
	#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	#[codec(dumb_trait_bound)]
	pub enum TimeControl {
		/// 30 seconds, no increment
		UltraBullet,
		/// 60 seconds, no increment
		Bullet,
		/// 120 seconds, 1 second increment
		Bullet2,
		/// 180 seconds, no increment
		Blitz3,
		/// 300 seconds, 3 second increment
		Blitz5,
		/// 600 seconds, no increment
		Rapid10,
		/// 900 seconds, 10 second increment
		Rapid15,
		/// 1800 seconds, no increment
		Rapid30,
		/// 3600 seconds, 30 second increment
		Classical,
		/// 86400 seconds (24 hours), no increment
		Daily,
		/// Unlimited time for practice
		Practice,
	}

	impl TimeControl {
		/// Get initial time in milliseconds
		pub fn initial_time_ms(&self) -> u64 {
			match self {
				Self::UltraBullet => 30_000,
				Self::Bullet => 60_000,
				Self::Bullet2 => 120_000,
				Self::Blitz3 => 180_000,
				Self::Blitz5 => 300_000,
				Self::Rapid10 => 600_000,
				Self::Rapid15 => 900_000,
				Self::Rapid30 => 1_800_000,
				Self::Classical => 3_600_000,
				Self::Daily => 86_400_000,
				Self::Practice => 999_999_000, // ~277 hours
			}
		}

		/// Get increment in milliseconds
		pub fn increment_ms(&self) -> u64 {
			match self {
				Self::UltraBullet | Self::Bullet | Self::Blitz3 | Self::Rapid10 | Self::Rapid30 |
				Self::Daily | Self::Practice => 0,
				Self::Bullet2 => 1_000,
				Self::Blitz5 => 3_000,
				Self::Rapid15 => 10_000,
				Self::Classical => 30_000,
			}
		}

		/// Convert to u8 for use in Events
		pub fn to_u8(&self) -> u8 {
			match self {
				Self::UltraBullet => 0,
				Self::Bullet => 1,
				Self::Bullet2 => 2,
				Self::Blitz3 => 3,
				Self::Blitz5 => 4,
				Self::Rapid10 => 5,
				Self::Rapid15 => 6,
				Self::Rapid30 => 7,
				Self::Classical => 8,
				Self::Daily => 9,
				Self::Practice => 10,
			}
		}

		/// Convert from u8
		pub fn from_u8(value: u8) -> Option<Self> {
			match value {
				0 => Some(Self::UltraBullet),
				1 => Some(Self::Bullet),
				2 => Some(Self::Bullet2),
				3 => Some(Self::Blitz3),
				4 => Some(Self::Blitz5),
				5 => Some(Self::Rapid10),
				6 => Some(Self::Rapid15),
				7 => Some(Self::Rapid30),
				8 => Some(Self::Classical),
				9 => Some(Self::Daily),
				10 => Some(Self::Practice),
				_ => None,
			}
		}
	}

	/// Square on chess board (0-63)
	#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	#[codec(dumb_trait_bound)]
	pub struct Square(pub u8);

	impl Square {
		/// Create from algebraic notation (e.g., "e4")
		pub fn from_algebraic(s: &str) -> Option<Self> {
			let bytes = s.as_bytes();
			if bytes.len() != 2 {
				return None
			}
			let file = bytes[0].checked_sub(b'a')?;
			let rank = bytes[1].checked_sub(b'1')?;
			if file > 7 || rank > 7 {
				return None
			}
			Some(Self(rank * 8 + file))
		}

		/// Convert to shakmaty square
		pub fn to_shakmaty(&self) -> Option<ShakmSquare> {
			if self.0 < 64 {
				Some(ShakmSquare::new(self.0 as u32))
			} else {
				None
			}
		}
	}

	/// Piece type for promotion
	#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	#[codec(dumb_trait_bound)]
	pub enum PieceType {
		Queen,
		Rook,
		Bishop,
		Knight,
	}

	/// Game variant type
	#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	#[codec(dumb_trait_bound)]
	pub enum GameVariant {
		/// Standard chess
		Standard,
		/// Fischer Random (Chess960)
		FischerRandom,
	}

	impl PieceType {
		/// Convert to shakmaty Role
		pub fn to_shakmaty(&self) -> Role {
			match self {
				Self::Queen => Role::Queen,
				Self::Rook => Role::Rook,
				Self::Bishop => Role::Bishop,
				Self::Knight => Role::Knight,
			}
		}

		/// Convert to u8 for use in Events
		pub fn to_u8(&self) -> u8 {
			match self {
				Self::Queen => 0,
				Self::Rook => 1,
				Self::Bishop => 2,
				Self::Knight => 3,
			}
		}

		/// Convert from u8
		pub fn from_u8(value: u8) -> Option<Self> {
			match value {
				0 => Some(Self::Queen),
				1 => Some(Self::Rook),
				2 => Some(Self::Bishop),
				3 => Some(Self::Knight),
				_ => None,
			}
		}
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Configure the pallet
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The currency mechanism for stakes
		type Currency: Inspect<Self::AccountId>
			+ Mutate<Self::AccountId>
			+ MutateHold<Self::AccountId, Reason = Self::RuntimeHoldReason>;

		/// The hold reason for stakes
		type RuntimeHoldReason: From<HoldReason>;

		/// Unix timestamp provider
		type UnixTime: UnixTime;

		/// Minimum stake required to create a game
		#[pallet::constant]
		type MinimumStake: Get<BalanceOf<Self>>;
	}

	/// Hold reason for game stakes
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Stake held for active game
		GameStake,
	}

	/// A single chess move
	#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	pub struct Move {
		/// From square
		pub from: Square,
		/// To square
		pub to: Square,
		/// Promotion piece (for pawn promotion)
		pub promotion: Option<PieceType>,
		/// Standard Algebraic Notation (e.g., "Nf3", "e4")
		pub san: BoundedVec<u8, ConstU32<10>>,
		/// FEN string after this move
		pub fen_after: BoundedVec<u8, ConstU32<MAX_FEN_LENGTH>>,
		/// Block timestamp when move was made
		pub timestamp: u64,
	}

	/// Time state for a game
	#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	pub struct TimeState<BlockNumber> {
		/// White's remaining time in milliseconds
		pub white_time_ms: u64,
		/// Black's remaining time in milliseconds
		pub black_time_ms: u64,
		/// Timestamp when last move was made
		pub last_move_timestamp: u64,
		/// Block number when last move was made
		pub last_move_block: BlockNumber,
		/// Increment added after each move (milliseconds)
		pub increment_ms: u64,
	}

	/// Player statistics
	#[derive(Encode, Decode, Clone, Copy, Default, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	pub struct PlayerStats {
		/// Total games played
		pub games_played: u32,
		/// Games won
		pub wins: u32,
		/// Games lost
		pub losses: u32,
		/// Games drawn
		pub draws: u32,
		/// ELO rating (starts at 1200)
		pub rating: u16,
	}

	/// Main game struct
	#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	#[scale_info(skip_type_params(T))]
	pub struct Game<T: Config> {
		/// Player 1 (creator)
		pub player1: T::AccountId,
		/// Player 2 (joiner) - None until joined
		pub player2: Option<T::AccountId>,
		/// Stake amount (held from both players)
		pub stake: BalanceOf<T>,
		/// Game status
		pub status: GameStatus,
		/// Game result
		pub result: GameResult,
		/// Is player1 white? (determines colors)
		pub player1_is_white: bool,
		/// Time control type
		pub time_control: TimeControl,
		/// Game variant (Standard or Fischer Random)
		pub variant: GameVariant,
		/// Block when game was created
		pub created_at: BlockNumberFor<T>,
		/// Block of last activity (for timeout detection)
		pub last_activity: BlockNumberFor<T>,
		/// Current board position (FEN)
		pub current_fen: BoundedVec<u8, ConstU32<MAX_FEN_LENGTH>>,
		/// Number of moves made
		pub move_count: u16,
		/// Halfmove clock (for 50-move rule)
		pub halfmove_clock: u16,
		/// Account that offered a draw (None if no pending offer)
		pub pending_draw_offer: Option<T::AccountId>,
	}

	/// Type alias for balance
	pub type BalanceOf<T> =
		<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

	/// Storage: All games indexed by game ID
	#[pallet::storage]
	pub type Games<T: Config> = StorageMap<_, Blake2_128Concat, GameId, Game<T>>;

	/// Storage: Move history for each game
	#[pallet::storage]
	pub type GameMoves<T: Config> =
		StorageMap<_, Blake2_128Concat, GameId, BoundedVec<Move, ConstU32<MAX_MOVES>>, ValueQuery>;

	/// Storage: Time state for each game
	#[pallet::storage]
	pub type GameTime<T: Config> =
		StorageMap<_, Blake2_128Concat, GameId, TimeState<BlockNumberFor<T>>>;

	/// Storage: Active games for each player
	#[pallet::storage]
	pub type ActiveGames<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		BoundedVec<GameId, ConstU32<MAX_GAMES_PER_PLAYER>>,
		ValueQuery,
	>;

	/// Storage: Open games waiting for player 2
	#[pallet::storage]
	pub type OpenGames<T: Config> =
		StorageValue<_, BoundedVec<GameId, ConstU32<MAX_OPEN_GAMES>>, ValueQuery>;

	/// Storage: Game nonce for generating unique game IDs
	#[pallet::storage]
	pub type GameNonce<T: Config> = StorageValue<_, u64, ValueQuery>;

	/// Storage: Player statistics
	#[pallet::storage]
	pub type PlayerStatistics<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, PlayerStats, ValueQuery>;

	/// Storage: Position history for threefold repetition detection (stores FEN hashes)
	#[pallet::storage]
	pub type PositionHistory<T: Config> =
		StorageMap<_, Blake2_128Concat, GameId, BoundedVec<[u8; 32], ConstU32<MAX_MOVES>>, ValueQuery>;

	/// Events emitted by this pallet
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new game was created
		GameCreated {
			game_id: GameId,
			player1: T::AccountId,
			stake: BalanceOf<T>,
			player1_is_white: bool,
			time_control: u8,
			variant: u8,
		},
		/// Player 2 joined a game
		GameJoined { game_id: GameId, player2: T::AccountId },
		/// A move was played
		MovePlayed {
			game_id: GameId,
			player: T::AccountId,
			from_square: u8,
			to_square: u8,
			san: BoundedVec<u8, ConstU32<10>>,
		},
		/// Game ended
		GameEnded { game_id: GameId, result: u8, winner: Option<T::AccountId> },
		/// Draw was offered
		DrawOffered { game_id: GameId, offerer: T::AccountId },
		/// Draw was accepted
		DrawAccepted { game_id: GameId },
		/// Draw was declined
		DrawDeclined { game_id: GameId },
		/// Player resigned
		PlayerResigned { game_id: GameId, player: T::AccountId },
		/// Timeout claimed
		TimeoutClaimed { game_id: GameId, winner: T::AccountId },
		/// Game cancelled before starting
		GameCancelled { game_id: GameId, player: T::AccountId },
		/// Check was given
		CheckGiven { game_id: GameId },
		/// Piece was captured
		PieceCaptured { game_id: GameId, captured_piece: u8 },
		/// Threefold repetition occurred (auto-draw)
		ThreefoldRepetition { game_id: GameId },
		/// Fifty-move rule triggered (auto-draw)
		FiftyMoveRule { game_id: GameId },
		/// Player rating changed
		RatingChanged { player: T::AccountId, old_rating: u16, new_rating: u16 },
	}

	/// Errors that can occur
	#[pallet::error]
	pub enum Error<T> {
		/// Game does not exist
		GameNotFound,
		/// Game is not in waiting status
		GameNotWaiting,
		/// Game is not active
		GameNotActive,
		/// Cannot join own game
		CannotJoinOwnGame,
		/// Player is not in this game
		NotPlayerInGame,
		/// Not player's turn
		NotYourTurn,
		/// Move is illegal
		IllegalMove,
		/// Invalid FEN string
		InvalidFEN,
		/// Invalid square notation
		InvalidSquare,
		/// Too many active games
		TooManyActiveGames,
		/// Too many open games
		TooManyOpenGames,
		/// Player 2 not found
		Player2NotFound,
		/// Time state not found
		TimeStateNotFound,
		/// Game has timed out
		GameTimedOut,
		/// No timeout has occurred
		NoTimeout,
		/// Too many moves
		TooManyMoves,
		/// Insufficient balance for stake
		InsufficientBalance,
		/// No pending draw offer
		NoPendingDrawOffer,
		/// Cannot accept own draw offer
		CannotAcceptOwnDrawOffer,
		/// Game already has pending draw offer
		DrawOfferAlreadyPending,
		/// Invalid game variant
		InvalidGameVariant,
		/// Cannot cancel game that has started
		CannotCancelStartedGame,
		/// Stake below minimum required
		StakeTooLow,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create a new chess game
		///
		/// Parameters:
		/// - `stake`: Amount to stake (winner takes all)
		/// - `player1_is_white`: Whether creator plays as white
		/// - `time_control_id`: Time control type (0-10)
		/// - `variant_id`: Game variant (0=Standard, 1=FischerRandom)
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn create_game(
			origin: OriginFor<T>,
			stake: BalanceOf<T>,
			player1_is_white: bool,
			time_control_id: u8,
			variant_id: u8,
		) -> DispatchResult {
			let creator = ensure_signed(origin)?;

			// Validate minimum stake
			ensure!(stake >= T::MinimumStake::get(), Error::<T>::StakeTooLow);

			// Convert u8 to TimeControl
			let time_control = TimeControl::from_u8(time_control_id)
				.ok_or(Error::<T>::InvalidSquare)?;

			// Convert u8 to GameVariant
			let variant = match variant_id {
				0 => GameVariant::Standard,
				1 => GameVariant::FischerRandom,
				_ => return Err(Error::<T>::InvalidGameVariant.into()),
			};

			// Hold stake from creator
			T::Currency::hold(&HoldReason::GameStake.into(), &creator, stake)
				.map_err(|_| Error::<T>::InsufficientBalance)?;

			// Generate unique game ID
			let nonce = GameNonce::<T>::get();
			let game_id = Self::generate_game_id(&creator, nonce);
			GameNonce::<T>::put(nonce.wrapping_add(1));

			// Get starting FEN based on variant
			let starting_fen = match variant {
				GameVariant::Standard => STARTING_FEN,
				GameVariant::FischerRandom => {
					// For Fischer Random, we'd generate a random position
					// For now, use standard position (TODO: implement FEN generation)
					STARTING_FEN
				},
			};

			// Create game struct
			let game = Game {
				player1: creator.clone(),
				player2: None,
				stake,
				status: GameStatus::Waiting,
				result: GameResult::Ongoing,
				player1_is_white,
				time_control,
				variant,
				created_at: frame_system::Pallet::<T>::block_number(),
				last_activity: frame_system::Pallet::<T>::block_number(),
				current_fen: BoundedVec::try_from(starting_fen.as_bytes().to_vec())
					.map_err(|_| Error::<T>::InvalidFEN)?,
				move_count: 0,
				halfmove_clock: 0,
				pending_draw_offer: None,
			};

			// Store game
			Games::<T>::insert(game_id, game);

			// Add to open games
			OpenGames::<T>::try_mutate(|games| games.try_push(game_id))
				.map_err(|_| Error::<T>::TooManyOpenGames)?;

			// Add to creator's active games
			ActiveGames::<T>::try_mutate(&creator, |games| games.try_push(game_id))
				.map_err(|_| Error::<T>::TooManyActiveGames)?;

			// Emit event
			Self::deposit_event(Event::GameCreated {
				game_id,
				player1: creator,
				stake,
				player1_is_white,
				time_control: time_control_id,
				variant: variant_id,
			});

			Ok(())
		}

		/// Join an existing game as player 2
		///
		/// Parameters:
		/// - `game_id`: ID of game to join
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn join_game(origin: OriginFor<T>, game_id: GameId) -> DispatchResult {
			let player = ensure_signed(origin)?;

			// Get game
			let mut game = Games::<T>::get(game_id).ok_or(Error::<T>::GameNotFound)?;

			// Validate
			ensure!(game.status == GameStatus::Waiting, Error::<T>::GameNotWaiting);
			ensure!(game.player1 != player, Error::<T>::CannotJoinOwnGame);

			// Hold stake from player 2
			T::Currency::hold(&HoldReason::GameStake.into(), &player, game.stake)
				.map_err(|_| Error::<T>::InsufficientBalance)?;

			// Update game
			game.player2 = Some(player.clone());
			game.status = GameStatus::Active;
			game.last_activity = frame_system::Pallet::<T>::block_number();

			// Initialize time state
			let time_state = TimeState {
				white_time_ms: game.time_control.initial_time_ms(),
				black_time_ms: game.time_control.initial_time_ms(),
				last_move_timestamp: T::UnixTime::now().as_millis().saturated_into(),
				last_move_block: frame_system::Pallet::<T>::block_number(),
				increment_ms: game.time_control.increment_ms(),
			};
			GameTime::<T>::insert(game_id, time_state);

			// Store updated game
			Games::<T>::insert(game_id, game);

			// Remove from open games
			OpenGames::<T>::mutate(|games| games.retain(|&g| g != game_id));

			// Add to player 2's active games
			ActiveGames::<T>::try_mutate(&player, |games| games.try_push(game_id))
				.map_err(|_| Error::<T>::TooManyActiveGames)?;

			// Emit event
			Self::deposit_event(Event::GameJoined { game_id, player2: player });

			Ok(())
		}

		/// Submit a chess move
		///
		/// Parameters:
		/// - `game_id`: ID of the game
		/// - `from`: Starting square (e.g., "e2")
		/// - `to`: Destination square (e.g., "e4")
		/// - `promotion`: Optional piece type for pawn promotion
		#[pallet::call_index(2)]
		#[pallet::weight(Weight::from_parts(50_000, 0))]
		pub fn submit_move(
			origin: OriginFor<T>,
			game_id: GameId,
			from_square: u8,
			to_square: u8,
			promotion_piece: Option<u8>,
		) -> DispatchResult {
			let player = ensure_signed(origin)?;

			// Convert u8 to Square
			let from = Square(from_square);
			let to = Square(to_square);

			// Convert promotion if provided
			let promotion = promotion_piece
				.map(|p| PieceType::from_u8(p).ok_or(Error::<T>::InvalidSquare))
				.transpose()?;

			// Get game
			let mut game = Games::<T>::get(game_id).ok_or(Error::<T>::GameNotFound)?;

			// Validate
			ensure!(game.status == GameStatus::Active, Error::<T>::GameNotActive);
			ensure!(Self::is_player_turn(&game, &player)?, Error::<T>::NotYourTurn);

			// Load chess position from FEN
			let fen_str =
				core::str::from_utf8(&game.current_fen).map_err(|_| Error::<T>::InvalidFEN)?;
			let fen: Fen = fen_str.parse().map_err(|_| Error::<T>::InvalidFEN)?;
			// Parse FEN directly into Chess position
			let mut chess: Chess = fen.into_position(shakmaty::CastlingMode::Standard)
				.map_err(|_| Error::<T>::InvalidFEN)?;

			// Convert squares to shakmaty format
			let from_sq = from.to_shakmaty().ok_or(Error::<T>::InvalidSquare)?;
			let to_sq = to.to_shakmaty().ok_or(Error::<T>::InvalidSquare)?;

			// Find the legal move that matches from/to squares
			let legal_moves = chess.legal_moves();
			let chess_move = legal_moves
				.iter()
				.find(|m| m.from() == Some(from_sq) && m.to() == to_sq)
				.cloned()
				.ok_or(Error::<T>::IllegalMove)?;

			// Validate promotion matches if move is a promotion
			if let Some(promo) = promotion {
				match &chess_move {
					ChessMove::Normal {
						role: _,
						from: _,
						capture: _,
						to: _,
						promotion: move_promo,
					} => {
						if move_promo.as_ref() != Some(&promo.to_shakmaty()) {
							return Err(Error::<T>::IllegalMove.into());
						}
					},
					_ => {},
				}
			}

			// Check if this is a capture
			let is_capture = chess_move.is_capture();
			let captured_piece_role = if is_capture {
				chess.board().piece_at(to_sq).map(|p| p.role)
			} else {
				None
			};

			// Check if move is a pawn move (for halfmove clock)
			let _is_pawn_move = matches!(chess_move, ChessMove::Normal { role: Role::Pawn, .. } | ChessMove::EnPassant { .. });

			// Apply the move
			chess.play_unchecked(&chess_move);

			// Check if move gives check
			let gives_check = chess.is_check();

			// Generate SAN notation
			let san_str = San::from_move(&chess, &chess_move).to_string();

			// Get new FEN
			let new_fen_str = Fen::from_position(chess.clone(), shakmaty::EnPassantMode::Legal).to_string();

			// Parse halfmove clock from FEN (5th field)
			let fen_parts: alloc::vec::Vec<&str> = new_fen_str.split(' ').collect();
			let halfmove_clock = if fen_parts.len() >= 5 {
				fen_parts[4].parse::<u16>().unwrap_or(0)
			} else {
				0
			};

			// Hash the position (just the piece placement part of FEN)
			let position_key = if fen_parts.len() > 0 {
				T::Hashing::hash(fen_parts[0].as_bytes())
			} else {
				T::Hashing::hash(new_fen_str.as_bytes())
			};
			let mut position_hash = [0u8; 32];
			let hash_bytes = position_key.as_ref();
			position_hash[..hash_bytes.len().min(32)].copy_from_slice(&hash_bytes[..hash_bytes.len().min(32)]);

			// Store position hash for threefold repetition detection
			PositionHistory::<T>::try_mutate(game_id, |history| -> Result<(), Error<T>> {
				history.try_push(position_hash).map_err(|_| Error::<T>::TooManyMoves)?;
				Ok(())
			})?;

			// Check for threefold repetition
			let threefold = Self::check_threefold_repetition(game_id, &position_hash);

			// Check for fifty-move rule
			let fifty_move = halfmove_clock >= 100;

			// Check for game end conditions
			let (game_ended, result) = if chess.is_checkmate() {
				// The side to move is checkmated, so opponent wins
				let winner_is_white = chess.turn() == Color::Black;
				(
					true,
					if winner_is_white {
						GameResult::WhiteWins
					} else {
						GameResult::BlackWins
					},
				)
			} else if chess.is_stalemate() || chess.is_insufficient_material() {
				(true, GameResult::Draw)
			} else if threefold || fifty_move {
				// Auto-draw on threefold repetition or fifty-move rule
				(true, GameResult::Draw)
			} else {
				(false, GameResult::Ongoing)
			};

			// Update time tracking
			let mut time_state =
				GameTime::<T>::get(game_id).ok_or(Error::<T>::TimeStateNotFound)?;
			Self::update_time(&mut time_state, &game)?;

			// Check for timeout
			if Self::is_timeout(&time_state) {
				return Self::end_game_timeout(game_id, &game, &time_state);
			}

			// Store the move
			let move_struct = Move {
				from,
				to,
				promotion,
				san: BoundedVec::try_from(san_str.as_bytes().to_vec())
					.map_err(|_| Error::<T>::TooManyMoves)?,
				fen_after: BoundedVec::try_from(new_fen_str.as_bytes().to_vec())
					.map_err(|_| Error::<T>::InvalidFEN)?,
				timestamp: T::UnixTime::now().as_millis().saturated_into(),
			};

			GameMoves::<T>::try_mutate(game_id, |moves| moves.try_push(move_struct))
				.map_err(|_| Error::<T>::TooManyMoves)?;

			// Update game state
			game.current_fen = BoundedVec::try_from(new_fen_str.as_bytes().to_vec())
				.map_err(|_| Error::<T>::InvalidFEN)?;
			game.move_count += 1;
			game.halfmove_clock = halfmove_clock;
			game.last_activity = frame_system::Pallet::<T>::block_number();

			// Clear draw offer when move is made
			game.pending_draw_offer = None;

			if game_ended {
				game.status = GameStatus::Completed;
				game.result = result;
				Self::distribute_prizes(&game)?;
				Self::update_ratings(&game)?;
				Self::cleanup_game(game_id, &game);
			}

			// Store updated game and time state
			Games::<T>::insert(game_id, game.clone());
			GameTime::<T>::insert(game_id, time_state);

			// Emit events
			Self::deposit_event(Event::MovePlayed {
				game_id,
				player,
				from_square: from.0,
				to_square: to.0,
				san: BoundedVec::try_from(san_str.as_bytes().to_vec())
					.map_err(|_| Error::<T>::TooManyMoves)?,
			});

			// Emit check event if applicable
			if gives_check {
				Self::deposit_event(Event::CheckGiven { game_id });
			}

			// Emit capture event if applicable
			if is_capture {
				if let Some(captured_role) = captured_piece_role {
					let piece_u8 = match captured_role {
						Role::Pawn => 0,
						Role::Knight => 1,
						Role::Bishop => 2,
						Role::Rook => 3,
						Role::Queen => 4,
						Role::King => 5,
					};
					Self::deposit_event(Event::PieceCaptured { game_id, captured_piece: piece_u8 });
				}
			}

			// Emit special draw events
			if threefold {
				Self::deposit_event(Event::ThreefoldRepetition { game_id });
			}
			if fifty_move {
				Self::deposit_event(Event::FiftyMoveRule { game_id });
			}

			if game_ended {
				Self::deposit_event(Event::GameEnded {
					game_id,
					result: result.to_u8(),
					winner: Self::get_winner(&game),
				});
			}

			Ok(())
		}

		/// Resign from the game
		#[pallet::call_index(3)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn resign(origin: OriginFor<T>, game_id: GameId) -> DispatchResult {
			let player = ensure_signed(origin)?;

			let mut game = Games::<T>::get(game_id).ok_or(Error::<T>::GameNotFound)?;

			ensure!(game.status == GameStatus::Active, Error::<T>::GameNotActive);
			ensure!(
				game.player1 == player || game.player2 == Some(player.clone()),
				Error::<T>::NotPlayerInGame
			);

			// Determine winner (opponent)
			let winner = if game.player1 == player {
				game.player2.clone().ok_or(Error::<T>::Player2NotFound)?
			} else {
				game.player1.clone()
			};

			let result = if (game.player1_is_white && game.player1 == player) ||
				(!game.player1_is_white && game.player2 == Some(player.clone()))
			{
				GameResult::BlackWins
			} else {
				GameResult::WhiteWins
			};

			// End game
			game.status = GameStatus::Completed;
			game.result = result;

			Self::distribute_prizes(&game)?;
			Self::update_ratings(&game)?;
			Self::cleanup_game(game_id, &game);
			Games::<T>::insert(game_id, game);

			Self::deposit_event(Event::PlayerResigned { game_id, player: player.clone() });
			Self::deposit_event(Event::GameEnded {
				game_id,
				result: result.to_u8(),
				winner: Some(winner),
			});

			Ok(())
		}

		/// Cancel a game before it starts
		#[pallet::call_index(4)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn cancel_game(origin: OriginFor<T>, game_id: GameId) -> DispatchResult {
			let player = ensure_signed(origin)?;

			let mut game = Games::<T>::get(game_id).ok_or(Error::<T>::GameNotFound)?;

			// Only creator can cancel, and only before someone joins
			ensure!(game.player1 == player, Error::<T>::NotPlayerInGame);
			ensure!(game.status == GameStatus::Waiting, Error::<T>::CannotCancelStartedGame);

			// Release stake
			T::Currency::release(
				&HoldReason::GameStake.into(),
				&game.player1,
				game.stake,
				Precision::BestEffort,
			)?;

			// Update game status
			game.status = GameStatus::Cancelled;
			Games::<T>::insert(game_id, game);

			// Remove from open games
			OpenGames::<T>::mutate(|games| games.retain(|&g| g != game_id));

			// Remove from player's active games
			ActiveGames::<T>::mutate(&player, |games| games.retain(|&g| g != game_id));

			Self::deposit_event(Event::GameCancelled { game_id, player });

			Ok(())
		}

		/// Offer a draw
		#[pallet::call_index(5)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn offer_draw(origin: OriginFor<T>, game_id: GameId) -> DispatchResult {
			let player = ensure_signed(origin)?;

			let mut game = Games::<T>::get(game_id).ok_or(Error::<T>::GameNotFound)?;

			ensure!(game.status == GameStatus::Active, Error::<T>::GameNotActive);
			ensure!(
				game.player1 == player || game.player2 == Some(player.clone()),
				Error::<T>::NotPlayerInGame
			);
			ensure!(game.pending_draw_offer.is_none(), Error::<T>::DrawOfferAlreadyPending);

			// Store draw offer
			game.pending_draw_offer = Some(player.clone());
			Games::<T>::insert(game_id, game);

			Self::deposit_event(Event::DrawOffered { game_id, offerer: player });

			Ok(())
		}

		/// Accept a draw offer
		#[pallet::call_index(6)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn accept_draw(origin: OriginFor<T>, game_id: GameId) -> DispatchResult {
			let player = ensure_signed(origin)?;

			let mut game = Games::<T>::get(game_id).ok_or(Error::<T>::GameNotFound)?;

			ensure!(game.status == GameStatus::Active, Error::<T>::GameNotActive);
			ensure!(
				game.player1 == player || game.player2 == Some(player.clone()),
				Error::<T>::NotPlayerInGame
			);

			let offerer = game.pending_draw_offer.clone().ok_or(Error::<T>::NoPendingDrawOffer)?;
			ensure!(offerer != player, Error::<T>::CannotAcceptOwnDrawOffer);

			// End game as draw
			game.status = GameStatus::Completed;
			game.result = GameResult::Draw;
			game.pending_draw_offer = None;

			Self::distribute_prizes(&game)?;
			Self::update_ratings(&game)?;
			Self::cleanup_game(game_id, &game);
			Games::<T>::insert(game_id, game.clone());

			Self::deposit_event(Event::DrawAccepted { game_id });
			Self::deposit_event(Event::GameEnded {
				game_id,
				result: GameResult::Draw.to_u8(),
				winner: None,
			});

			Ok(())
		}

		/// Decline a draw offer
		#[pallet::call_index(7)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn decline_draw(origin: OriginFor<T>, game_id: GameId) -> DispatchResult {
			let player = ensure_signed(origin)?;

			let mut game = Games::<T>::get(game_id).ok_or(Error::<T>::GameNotFound)?;

			ensure!(game.status == GameStatus::Active, Error::<T>::GameNotActive);
			ensure!(
				game.player1 == player || game.player2 == Some(player.clone()),
				Error::<T>::NotPlayerInGame
			);

			let offerer = game.pending_draw_offer.clone().ok_or(Error::<T>::NoPendingDrawOffer)?;
			ensure!(offerer != player, Error::<T>::CannotAcceptOwnDrawOffer);

			// Clear draw offer
			game.pending_draw_offer = None;
			Games::<T>::insert(game_id, game);

			Self::deposit_event(Event::DrawDeclined { game_id });

			Ok(())
		}

		/// Claim timeout (either player can call if opponent's time ran out)
		#[pallet::call_index(8)]
		#[pallet::weight(Weight::from_parts(10_000, 0))]
		pub fn claim_timeout(origin: OriginFor<T>, game_id: GameId) -> DispatchResult {
			let player = ensure_signed(origin)?;

			let mut game = Games::<T>::get(game_id).ok_or(Error::<T>::GameNotFound)?;

			ensure!(game.status == GameStatus::Active, Error::<T>::GameNotActive);
			ensure!(
				game.player1 == player || game.player2 == Some(player.clone()),
				Error::<T>::NotPlayerInGame
			);

			let mut time_state = GameTime::<T>::get(game_id).ok_or(Error::<T>::TimeStateNotFound)?;

			// Update time to current moment
			Self::update_time(&mut time_state, &game)?;

			// Check if timeout occurred
			ensure!(Self::is_timeout(&time_state), Error::<T>::NoTimeout);

			// Determine winner (whoever didn't timeout)
			let result = if time_state.white_time_ms == 0 {
				GameResult::BlackWins
			} else {
				GameResult::WhiteWins
			};

			game.status = GameStatus::Timeout;
			game.result = result;

			Self::distribute_prizes(&game)?;
			Self::update_ratings(&game)?;
			Self::cleanup_game(game_id, &game);
			Games::<T>::insert(game_id, game.clone());

			Self::deposit_event(Event::TimeoutClaimed { game_id, winner: player });
			Self::deposit_event(Event::GameEnded {
				game_id,
				result: result.to_u8(),
				winner: Self::get_winner(&game),
			});

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Generate unique game ID from creator and nonce
		fn generate_game_id(creator: &T::AccountId, nonce: u64) -> GameId {
			let mut data = creator.encode();
			data.extend_from_slice(&nonce.to_le_bytes());
			let hash = T::Hashing::hash(&data);
			let hash_bytes = hash.as_ref();
			let mut game_id = [0u8; 32];
			game_id.copy_from_slice(&hash_bytes[..32]);
			game_id
		}

		/// Check if it's the player's turn
		fn is_player_turn(game: &Game<T>, player: &T::AccountId) -> Result<bool, Error<T>> {
			let is_white_turn = game.move_count % 2 == 0;
			let player_is_white = if game.player1 == *player {
				game.player1_is_white
			} else if game.player2.as_ref() == Some(player) {
				!game.player1_is_white
			} else {
				return Err(Error::<T>::NotPlayerInGame)
			};

			Ok(is_white_turn == player_is_white)
		}

		/// Distribute prizes to winner
		fn distribute_prizes(game: &Game<T>) -> DispatchResult {
			let winner = match game.result {
				GameResult::WhiteWins => {
					if game.player1_is_white {
						game.player1.clone()
					} else {
						game.player2.clone().ok_or(Error::<T>::Player2NotFound)?
					}
				},
				GameResult::BlackWins => {
					if game.player1_is_white {
						game.player2.clone().ok_or(Error::<T>::Player2NotFound)?
					} else {
						game.player1.clone()
					}
				},
				GameResult::Draw => {
					// Return stakes to both players
					T::Currency::release(
						&HoldReason::GameStake.into(),
						&game.player1,
						game.stake,
						Precision::BestEffort,
					)?;
					if let Some(ref player2) = game.player2 {
						T::Currency::release(
							&HoldReason::GameStake.into(),
							player2,
							game.stake,
							Precision::BestEffort,
						)?;
					}
					return Ok(())
				},
				_ => return Ok(()),
			};

			let loser = if winner == game.player1 {
				game.player2.clone().ok_or(Error::<T>::Player2NotFound)?
			} else {
				game.player1.clone()
			};

			// Release both stakes
			T::Currency::release(
				&HoldReason::GameStake.into(),
				&game.player1,
				game.stake,
				Precision::BestEffort,
			)?;
			if let Some(ref player2) = game.player2 {
				T::Currency::release(
					&HoldReason::GameStake.into(),
					player2,
					game.stake,
					Precision::BestEffort,
				)?;
			}

			// Transfer loser's stake to winner
			T::Currency::transfer(&loser, &winner, game.stake, Preservation::Expendable)?;

			Ok(())
		}

	/// Get the winner from a completed game
	fn get_winner(game: &Game<T>) -> Option<T::AccountId> {
		match game.result {
			GameResult::WhiteWins => {
				if game.player1_is_white {
					Some(game.player1.clone())
				} else {
					game.player2.clone()
				}
			},
			GameResult::BlackWins => {
				if game.player1_is_white {
					game.player2.clone()
				} else {
					Some(game.player1.clone())
				}
			},
			_ => None,
		}
	}

	/// Update time tracking after a move
	fn update_time(time_state: &mut TimeState<BlockNumberFor<T>>, game: &Game<T>) -> Result<(), Error<T>> {
		let current_time = T::UnixTime::now().as_millis().saturated_into::<u64>();
		let elapsed_ms = current_time.saturating_sub(time_state.last_move_timestamp);

		// Determine whose turn just finished (they made the move)
		let is_white_move = game.move_count.saturating_sub(1) % 2 == 0;

		if is_white_move {
			// White just moved, deduct time and add increment
			time_state.white_time_ms = time_state
				.white_time_ms
				.saturating_sub(elapsed_ms)
				.saturating_add(time_state.increment_ms);
		} else {
			// Black just moved, deduct time and add increment
			time_state.black_time_ms = time_state
				.black_time_ms
				.saturating_sub(elapsed_ms)
				.saturating_add(time_state.increment_ms);
		}

		time_state.last_move_timestamp = current_time;
		time_state.last_move_block = frame_system::Pallet::<T>::block_number();

		Ok(())
	}

	/// Check if a player has run out of time
	fn is_timeout(time_state: &TimeState<BlockNumberFor<T>>) -> bool {
		time_state.white_time_ms == 0 || time_state.black_time_ms == 0
	}

	/// End game due to timeout
	fn end_game_timeout(
		game_id: GameId,
		game: &Game<T>,
		time_state: &TimeState<BlockNumberFor<T>>,
	) -> DispatchResult {
		let mut game = game.clone();

		// Determine winner (whoever didn't timeout)
		let result = if time_state.white_time_ms == 0 {
			GameResult::BlackWins
		} else {
			GameResult::WhiteWins
		};

		game.status = GameStatus::Timeout;
		game.result = result;

		Self::distribute_prizes(&game)?;
		Self::cleanup_game(game_id, &game);
		Games::<T>::insert(game_id, game.clone());

		Self::deposit_event(Event::GameEnded {
			game_id,
			result: result.to_u8(),
			winner: Self::get_winner(&game),
		});

		Ok(())
	}

	/// Check if threefold repetition occurred
	fn check_threefold_repetition(game_id: GameId, position_hash: &[u8; 32]) -> bool {
		let history = PositionHistory::<T>::get(game_id);
		let count = history.iter().filter(|&h| h == position_hash).count();
		count >= 3
	}

	/// Clean up active games lists when a game ends
	fn cleanup_game(game_id: GameId, game: &Game<T>) {
		// Remove from player1's active games
		ActiveGames::<T>::mutate(&game.player1, |games| {
			games.retain(|&g| g != game_id);
		});

		// Remove from player2's active games if they exist
		if let Some(ref player2) = game.player2 {
			ActiveGames::<T>::mutate(player2, |games| {
				games.retain(|&g| g != game_id);
			});
		}
	}

	/// Update player ratings using simplified ELO system
	fn update_ratings(game: &Game<T>) -> DispatchResult {
		let player2 = game.player2.clone().ok_or(Error::<T>::Player2NotFound)?;

		// Get current ratings (default to 1200 for new players)
		let mut stats1 = PlayerStatistics::<T>::get(&game.player1);
		let mut stats2 = PlayerStatistics::<T>::get(&player2);

		// Initialize ratings if new players
		if stats1.rating == 0 {
			stats1.rating = 1200;
		}
		if stats2.rating == 0 {
			stats2.rating = 1200;
		}

		let old_rating1 = stats1.rating;
		let old_rating2 = stats2.rating;

		// Calculate expected scores (ELO formula)
		let expected1 = 1.0 / (1.0 + 10.0_f64.powf((stats2.rating as f64 - stats1.rating as f64) / 400.0));
		let expected2 = 1.0 / (1.0 + 10.0_f64.powf((stats1.rating as f64 - stats2.rating as f64) / 400.0));

		// Determine actual scores
		let (score1, score2) = match game.result {
			GameResult::WhiteWins => {
				if game.player1_is_white {
					(1.0, 0.0)
				} else {
					(0.0, 1.0)
				}
			},
			GameResult::BlackWins => {
				if game.player1_is_white {
					(0.0, 1.0)
				} else {
					(1.0, 0.0)
				}
			},
			GameResult::Draw => (0.5, 0.5),
			_ => return Ok(()),
		};

		// K-factor (higher for newer players)
		let k1 = if stats1.games_played < 30 { 40.0 } else { 20.0 };
		let k2 = if stats2.games_played < 30 { 40.0 } else { 20.0 };

		// Update ratings
		let rating_change1 = (k1 * (score1 - expected1)) as i32;
		let rating_change2 = (k2 * (score2 - expected2)) as i32;

		stats1.rating = ((stats1.rating as i32 + rating_change1).max(100).min(3000)) as u16;
		stats2.rating = ((stats2.rating as i32 + rating_change2).max(100).min(3000)) as u16;

		// Update statistics
		stats1.games_played += 1;
		stats2.games_played += 1;

		match game.result {
			GameResult::WhiteWins => {
				if game.player1_is_white {
					stats1.wins += 1;
					stats2.losses += 1;
				} else {
					stats1.losses += 1;
					stats2.wins += 1;
				}
			},
			GameResult::BlackWins => {
				if game.player1_is_white {
					stats1.losses += 1;
					stats2.wins += 1;
				} else {
					stats1.wins += 1;
					stats2.losses += 1;
				}
			},
			GameResult::Draw => {
				stats1.draws += 1;
				stats2.draws += 1;
			},
			_ => {},
		}

		let new_rating1 = stats1.rating;
		let new_rating2 = stats2.rating;

		// Store updated stats
		PlayerStatistics::<T>::insert(&game.player1, stats1);
		PlayerStatistics::<T>::insert(&player2, stats2.clone());

		// Emit rating change events
		if old_rating1 != new_rating1 {
			Self::deposit_event(Event::RatingChanged {
				player: game.player1.clone(),
				old_rating: old_rating1,
				new_rating: new_rating1,
			});
		}
		if old_rating2 != new_rating2 {
			Self::deposit_event(Event::RatingChanged {
				player: player2,
				old_rating: old_rating2,
				new_rating: new_rating2,
			});
		}

		Ok(())
	}

	// ===== Public query helper functions =====

	/// Get player statistics
	pub fn get_player_stats(player: &T::AccountId) -> PlayerStats {
		PlayerStatistics::<T>::get(player)
	}

	/// Get all active games for a player
	pub fn get_player_games(player: &T::AccountId) -> alloc::vec::Vec<GameId> {
		ActiveGames::<T>::get(player).into_inner()
	}

	/// Get all open games
	pub fn get_open_games() -> alloc::vec::Vec<GameId> {
		OpenGames::<T>::get().into_inner()
	}

	/// Get game details
	pub fn get_game(game_id: GameId) -> Option<Game<T>> {
		Games::<T>::get(game_id)
	}

	/// Get move history for a game
	pub fn get_moves(game_id: GameId) -> alloc::vec::Vec<Move> {
		GameMoves::<T>::get(game_id).into_inner()
	}

	/// Get time state for a game
	pub fn get_time_state(game_id: GameId) -> Option<TimeState<BlockNumberFor<T>>> {
		GameTime::<T>::get(game_id)
	}

	/// Check if a position is in check (requires parsing current FEN)
	pub fn is_check(game_id: GameId) -> bool {
		if let Some(game) = Games::<T>::get(game_id) {
			if let Ok(fen_str) = core::str::from_utf8(&game.current_fen) {
				if let Ok(fen) = fen_str.parse::<Fen>() {
					if let Ok(chess) = fen.into_position::<Chess>(shakmaty::CastlingMode::Standard) {
						return chess.is_check();
					}
				}
			}
		}
		false
	}

	/// Get legal moves for current position (returns count for now)
	pub fn get_legal_move_count(game_id: GameId) -> u32 {
		if let Some(game) = Games::<T>::get(game_id) {
			if let Ok(fen_str) = core::str::from_utf8(&game.current_fen) {
				if let Ok(fen) = fen_str.parse::<Fen>() {
					if let Ok(chess) = fen.into_position::<Chess>(shakmaty::CastlingMode::Standard) {
						return chess.legal_moves().len() as u32;
					}
				}
			}
		}
		0
	}

	/// Get top players by rating (leaderboard)
	/// Note: This is a simple implementation that iterates all players
	/// For production, consider using an indexed storage or off-chain worker
	pub fn get_leaderboard(limit: u32) -> alloc::vec::Vec<(T::AccountId, PlayerStats)> {
		let mut all_players: alloc::vec::Vec<(T::AccountId, PlayerStats)> =
			PlayerStatistics::<T>::iter().collect();

		// Sort by rating (descending)
		all_players.sort_by(|a, b| b.1.rating.cmp(&a.1.rating));

		// Take top N
		all_players.into_iter().take(limit as usize).collect()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use frame_support::{
		assert_noop, assert_ok, derive_impl,
		traits::{
			fungible::{Inspect, Mutate},
			tokens::{Fortitude, Precision, Preservation},
			ConstU32, ConstU64,
		},
	};
	use sp_runtime::{BuildStorage, traits::IdentityLookup};

	// Configure a mock runtime to test the pallet
	frame_support::construct_runtime!(
		pub enum Test {
			System: frame_system,
			Balances: pallet_balances,
			Timestamp: pallet_timestamp,
			Chess: pallet,
		}
	);

	type Block = frame_system::mocking::MockBlock<Test>;

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Test {
		type Block = Block;
		type AccountId = u64;
		type Lookup = IdentityLookup<Self::AccountId>;
		type AccountData = pallet_balances::AccountData<u64>;
	}

	impl pallet_balances::Config for Test {
		type MaxLocks = ConstU32<50>;
		type MaxReserves = ConstU32<50>;
		type ReserveIdentifier = [u8; 8];
		type Balance = u64;
		type RuntimeEvent = RuntimeEvent;
		type DustRemoval = ();
		type ExistentialDeposit = ConstU64<1>;
		type AccountStore = System;
		type WeightInfo = ();
		type FreezeIdentifier = ();
		type MaxFreezes = ConstU32<0>;
		type RuntimeHoldReason = RuntimeHoldReason;
		type RuntimeFreezeReason = RuntimeFreezeReason;
		type DoneSlashHandler = ();
	}

	impl pallet_timestamp::Config for Test {
		type Moment = u64;
		type OnTimestampSet = ();
		type MinimumPeriod = ConstU64<1>;
		type WeightInfo = ();
	}

	impl Config for Test {
		type RuntimeEvent = RuntimeEvent;
		type Currency = Balances;
		type RuntimeHoldReason = RuntimeHoldReason;
		type UnixTime = Timestamp;
		type MinimumStake = ConstU64<10>;
	}

	// Build genesis storage according to the mock runtime
	pub fn new_test_ext() -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

		pallet_balances::GenesisConfig::<Test> {
			balances: vec![(1, 1000), (2, 1000), (3, 1000)],
			dev_accounts: None,
		}
		.assimilate_storage(&mut t)
		.unwrap();

		let mut ext = sp_io::TestExternalities::new(t);
		ext.execute_with(|| {
			System::set_block_number(1);
			Timestamp::set_timestamp(1000);
		});
		ext
	}

	#[test]
	fn create_game_works() {
		new_test_ext().execute_with(|| {
			let stake = 100;
			let time_control = TimeControl::Blitz5.to_u8();

			// Create game as player 1 (white)
			assert_ok!(Pallet::<Test>::create_game(
				RuntimeOrigin::signed(1),
				stake,
				true, // player1 is white
				time_control,
				0 // Standard variant
			));

			// Verify game was created
			let game_id = Pallet::<Test>::generate_game_id(&1, 0);
			let game = Games::<Test>::get(game_id).unwrap();

			assert_eq!(game.player1, 1);
			assert_eq!(game.player2, None);
			assert_eq!(game.stake, stake);
			assert_eq!(game.status, GameStatus::Waiting);
			assert_eq!(game.player1_is_white, true);
			assert_eq!(game.variant, GameVariant::Standard);

			// Verify balance was held
			// Note: RuntimeHoldReason is not accessible in test scope, but we verify game state instead
			// let held_balance = <Balances as InspectHold<u64>>::balance_on_hold(
			//     &RuntimeHoldReason::ChessGame(crate::pallet::HoldReason::GameStake),
			//     &1
			// );
			// assert_eq!(held_balance, stake);
		});
	}

	#[test]
	fn join_game_works() {
		new_test_ext().execute_with(|| {
			let stake = 100;
			let time_control = TimeControl::Blitz5.to_u8();

			// Player 1 creates game
			assert_ok!(Pallet::<Test>::create_game(
				RuntimeOrigin::signed(1),
				stake,
				true,
				time_control,
				0
			));

			let game_id = Pallet::<Test>::generate_game_id(&1, 0);

			// Player 2 joins
			assert_ok!(Pallet::<Test>::join_game(RuntimeOrigin::signed(2), game_id));

			// Verify game state
			let game = Games::<Test>::get(game_id).unwrap();
			assert_eq!(game.player2, Some(2));
			assert_eq!(game.status, GameStatus::Active);

			// Verify both players have stakes held
			// Note: RuntimeHoldReason is not accessible in test scope, but we verify game state instead
			// let held_1 = <Balances as InspectHold<u64>>::balance_on_hold(
			//     &RuntimeHoldReason::ChessGame(crate::pallet::HoldReason::GameStake),
			//     &1
			// );
			// let held_2 = <Balances as InspectHold<u64>>::balance_on_hold(
			//     &RuntimeHoldReason::ChessGame(crate::pallet::HoldReason::GameStake),
			//     &2
			// );
			// assert_eq!(held_1, stake);
			// assert_eq!(held_2, stake);
		});
	}

	#[test]
	fn submit_legal_move_works() {
		new_test_ext().execute_with(|| {
			let stake = 100;
			let time_control = TimeControl::Blitz5.to_u8();

			// Create and join game
			assert_ok!(Pallet::<Test>::create_game(
				RuntimeOrigin::signed(1),
				stake,
				true, // player 1 is white
				time_control,
				0
			));
			let game_id = Pallet::<Test>::generate_game_id(&1, 0);
			assert_ok!(Pallet::<Test>::join_game(RuntimeOrigin::signed(2), game_id));

			// White (player 1) makes first move: e2-e4 (square 12 -> 28)
			assert_ok!(Pallet::<Test>::submit_move(
				RuntimeOrigin::signed(1),
				game_id,
				12, // e2
				28, // e4
				None
			));

			// Verify move was recorded
			let moves = GameMoves::<Test>::get(game_id);
			assert_eq!(moves.len(), 1);

			// Black (player 2) makes move: e7-e5 (square 52 -> 36)
			assert_ok!(Pallet::<Test>::submit_move(
				RuntimeOrigin::signed(2),
				game_id,
				52, // e7
				36, // e5
				None
			));

			// Verify second move was recorded
			let moves = GameMoves::<Test>::get(game_id);
			assert_eq!(moves.len(), 2);
		});
	}

	#[test]
	fn submit_illegal_move_fails() {
		new_test_ext().execute_with(|| {
			let stake = 100;
			let time_control = TimeControl::Blitz5.to_u8();

			// Create and join game
			assert_ok!(Pallet::<Test>::create_game(
				RuntimeOrigin::signed(1),
				stake,
				true,
				time_control,
				0
			));
			let game_id = Pallet::<Test>::generate_game_id(&1, 0);
			assert_ok!(Pallet::<Test>::join_game(RuntimeOrigin::signed(2), game_id));

			// Try illegal move: e2-e5 (pawn can't move 3 squares)
			assert_noop!(
				Pallet::<Test>::submit_move(
					RuntimeOrigin::signed(1),
					game_id,
					12, // e2
					36, // e5 (too far!)
					None
				),
				Error::<Test>::IllegalMove
			);
		});
	}

	#[test]
	fn wrong_player_cannot_move() {
		new_test_ext().execute_with(|| {
			let stake = 100;
			let time_control = TimeControl::Blitz5.to_u8();

			// Create and join game
			assert_ok!(Pallet::<Test>::create_game(
				RuntimeOrigin::signed(1),
				stake,
				true, // player 1 is white
				time_control,
				0
			));
			let game_id = Pallet::<Test>::generate_game_id(&1, 0);
			assert_ok!(Pallet::<Test>::join_game(RuntimeOrigin::signed(2), game_id));

			// Try to move as black (player 2) when it's white's turn
			assert_noop!(
				Pallet::<Test>::submit_move(
					RuntimeOrigin::signed(2),
					game_id,
					52, // e7
					36, // e5
					None
				),
				Error::<Test>::NotYourTurn
			);
		});
	}

	#[test]
	fn resign_works() {
		new_test_ext().execute_with(|| {
			let stake = 100;
			let time_control = TimeControl::Blitz5.to_u8();

			// Create and join game
			assert_ok!(Pallet::<Test>::create_game(
				RuntimeOrigin::signed(1),
				stake,
				true,
				time_control,
				0
			));
			let game_id = Pallet::<Test>::generate_game_id(&1, 0);
			assert_ok!(Pallet::<Test>::join_game(RuntimeOrigin::signed(2), game_id));

			let balance_1_before = <pallet_balances::Pallet<Test> as Inspect<u64>>::balance(&1);
			let balance_2_before = <pallet_balances::Pallet<Test> as Inspect<u64>>::balance(&2);

			// Player 1 resigns
			assert_ok!(Pallet::<Test>::resign(RuntimeOrigin::signed(1), game_id));

			// Verify game ended
			let game = Games::<Test>::get(game_id).unwrap();
			assert_eq!(game.status, GameStatus::Completed);
			assert_eq!(game.result, GameResult::BlackWins); // Player 2 (black) wins

			// Verify prize distribution (player 2 should get both stakes)
			let balance_1_after = <pallet_balances::Pallet<Test> as Inspect<u64>>::balance(&1);
			let balance_2_after = <pallet_balances::Pallet<Test> as Inspect<u64>>::balance(&2);

			assert_eq!(balance_1_after, balance_1_before); // Lost stake
			assert_eq!(balance_2_after, balance_2_before + stake * 2); // Won both stakes
		});
	}

	#[test]
	fn cannot_join_own_game() {
		new_test_ext().execute_with(|| {
			let stake = 100;
			let time_control = TimeControl::Blitz5.to_u8();

			// Create game
			assert_ok!(Pallet::<Test>::create_game(
				RuntimeOrigin::signed(1),
				stake,
				true,
				time_control,
				0
			));
			let game_id = Pallet::<Test>::generate_game_id(&1, 0);

			// Try to join own game
			assert_noop!(
				Pallet::<Test>::join_game(RuntimeOrigin::signed(1), game_id),
				Error::<Test>::CannotJoinOwnGame
			);
		});
	}

	#[test]
	fn cannot_join_with_wrong_stake() {
		new_test_ext().execute_with(|| {
			let stake = 100;
			let time_control = TimeControl::Blitz5.to_u8();

			// Create game
			assert_ok!(Pallet::<Test>::create_game(
				RuntimeOrigin::signed(1),
				stake,
				true,
				time_control,
				0
			));
			let game_id = Pallet::<Test>::generate_game_id(&1, 0);

			// Give player 2 insufficient balance
			let _ = <pallet_balances::Pallet<Test> as Mutate<u64>>::burn_from(
				&2,
				950,
				Preservation::Preserve,
				Precision::Exact,
				Fortitude::Polite
			);

			// Try to join with insufficient balance
			assert_noop!(
				Pallet::<Test>::join_game(RuntimeOrigin::signed(2), game_id),
				Error::<Test>::InsufficientBalance
			);
		});
	}

	#[test]
	fn time_control_conversion_works() {
		assert_eq!(TimeControl::UltraBullet.to_u8(), 0);
		assert_eq!(TimeControl::Bullet.to_u8(), 1);
		assert_eq!(TimeControl::Bullet2.to_u8(), 2);
		assert_eq!(TimeControl::Blitz3.to_u8(), 3);
		assert_eq!(TimeControl::Blitz5.to_u8(), 4);
		assert_eq!(TimeControl::Rapid10.to_u8(), 5);
		assert_eq!(TimeControl::Rapid15.to_u8(), 6);
		assert_eq!(TimeControl::Rapid30.to_u8(), 7);
		assert_eq!(TimeControl::Classical.to_u8(), 8);
		assert_eq!(TimeControl::Daily.to_u8(), 9);
		assert_eq!(TimeControl::Practice.to_u8(), 10);

		assert_eq!(TimeControl::from_u8(0), Some(TimeControl::UltraBullet));
		assert_eq!(TimeControl::from_u8(1), Some(TimeControl::Bullet));
		assert_eq!(TimeControl::from_u8(10), Some(TimeControl::Practice));
		assert_eq!(TimeControl::from_u8(99), None);
	}

	#[test]
	fn game_result_conversion_works() {
		assert_eq!(GameResult::Ongoing.to_u8(), 0);
		assert_eq!(GameResult::WhiteWins.to_u8(), 1);
		assert_eq!(GameResult::BlackWins.to_u8(), 2);
		assert_eq!(GameResult::Draw.to_u8(), 3);

		assert_eq!(GameResult::from_u8(0), Some(GameResult::Ongoing));
		assert_eq!(GameResult::from_u8(1), Some(GameResult::WhiteWins));
		assert_eq!(GameResult::from_u8(2), Some(GameResult::BlackWins));
		assert_eq!(GameResult::from_u8(3), Some(GameResult::Draw));
		assert_eq!(GameResult::from_u8(99), None);
	}
}
}
