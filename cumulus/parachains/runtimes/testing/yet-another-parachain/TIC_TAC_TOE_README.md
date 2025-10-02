# Tic-Tac-Toe Pallet

A decentralized tic-tac-toe game implementation for two players.

## Overview

This pallet allows players to create and play tic-tac-toe games on-chain. Each game is stored on the blockchain, and all moves are transparent and verifiable.

## Features

- **Create Games**: Any account can challenge another account to a game
- **Make Moves**: Players take turns making moves on a 3x3 grid
- **Automatic Win Detection**: The pallet automatically detects wins, draws, and game-over states
- **Event Emission**: All game actions emit events for easy tracking

## Game Board

The board is represented as a 3x3 grid with positions numbered 0-8:

```
0 | 1 | 2
---------
3 | 4 | 5
---------
6 | 7 | 8
```

## Extrinsics

### `create_game(opponent: AccountId)`

Creates a new game between the caller (Player X) and the specified opponent (Player O).

- Player X always goes first
- Returns a unique `game_id` that can be used to reference the game

**Example**:
```rust
// Alice challenges Bob to a game
TicTacToe::create_game(Origin::signed(ALICE), BOB)
```

### `make_move(game_id: u32, position: u8)`

Makes a move in an existing game.

- `game_id`: The ID of the game (returned from `create_game`)
- `position`: A number from 0-8 representing the board position

**Example**:
```rust
// Alice (Player X) makes a move at position 4 (center)
TicTacToe::make_move(Origin::signed(ALICE), 0, 4)

// Bob (Player O) makes a move at position 0 (top-left)
TicTacToe::make_move(Origin::signed(BOB), 0, 0)
```

## Events

### `GameCreated { game_id, player_x, player_o }`

Emitted when a new game is created.

### `MoveMade { game_id, player, position }`

Emitted when a player makes a move.

### `GameEnded { game_id, state }`

Emitted when a game ends with one of the following states:
- `XWon`: Player X won
- `OWon`: Player O won
- `Draw`: The game ended in a draw

## Errors

- `GameNotFound`: The specified game ID doesn't exist
- `NotYourTurn`: It's not the caller's turn
- `InvalidPosition`: Position must be between 0-8
- `CellOccupied`: The chosen position is already taken
- `GameEnded`: The game has already finished
- `CannotPlayAgainstSelf`: Cannot create a game against yourself
- `NotAPlayer`: The caller is not a player in this game

## Storage

### `Games`

A mapping from `game_id` to `Game` struct containing:
- `player_x`: AccountId of Player X
- `player_o`: AccountId of Player O
- `x_turn`: Boolean indicating if it's X's turn
- `board`: Array of 9 cells representing the game state
- `state`: Current game state (InProgress, XWon, OWon, or Draw)

### `NextGameId`

Counter for generating unique game IDs.

## Example Game Flow

```rust
// 1. Alice creates a game against Bob (game_id = 0)
TicTacToe::create_game(Origin::signed(ALICE), BOB)?;

// 2. Alice (X) plays center
TicTacToe::make_move(Origin::signed(ALICE), 0, 4)?;

// 3. Bob (O) plays top-left
TicTacToe::make_move(Origin::signed(BOB), 0, 0)?;

// 4. Alice (X) plays top-right
TicTacToe::make_move(Origin::signed(ALICE), 0, 2)?;

// 5. Bob (O) plays middle-left
TicTacToe::make_move(Origin::signed(BOB), 0, 3)?;

// 6. Alice (X) plays bottom-left (wins with diagonal 2-4-6)
TicTacToe::make_move(Origin::signed(ALICE), 0, 6)?;
// Event: GameEnded { game_id: 0, state: XWon }
```

## Integration

The pallet has been integrated into the yet-another-parachain runtime at pallet index 42.

To use it:
1. The pallet is already configured in the runtime
2. Use the PolkadotJS Apps UI or construct extrinsics programmatically
3. Query game state using the `games(game_id)` storage function 