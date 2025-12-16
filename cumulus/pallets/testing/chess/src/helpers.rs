// Helper functions for pallet-chess-game
// These should be added to the impl<T: Config> Pallet<T> block in lib.rs

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
	let is_white_move = (game.move_count - 1) % 2 == 0;

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
	Games::<T>::insert(game_id, game.clone());

	Self::deposit_event(Event::GameEnded {
		game_id,
		result,
		winner: Self::get_winner(&game),
	});

	Ok(())
}
