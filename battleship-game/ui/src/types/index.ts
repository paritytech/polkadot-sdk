export interface Position {
  x: number;
  y: number;
}

export type Orientation = "horizontal" | "vertical";

export type CellState = "empty" | "ship" | "hit" | "miss";

export interface Cell {
  state: CellState;
  shipId: string | null;
}

export interface ShipDefinition {
  id: string;
  name: string;
  size: number;
}

export interface PlacedShip {
  definition: ShipDefinition;
  position: Position;
  orientation: Orientation;
  hits: Set<number>;
}

export type GamePhase = "placement" | "battle" | "gameOver";

export type Turn = "player" | "opponent";

export interface GameState {
  phase: GamePhase;
  turn: Turn;
  winner: Turn | null;
}

export const GRID_SIZE = 10;
export const TILE_WIDTH = 64;
export const TILE_HEIGHT = 32;
export const BOARD_OFFSET_X = 320;
export const BOARD_OFFSET_Y = 40;

export const SHIP_DEFINITIONS: ShipDefinition[] = [
  { id: "carrier", name: "Carrier", size: 5 },
  { id: "battleship", name: "Battleship", size: 4 },
  { id: "cruiser", name: "Cruiser", size: 3 },
  { id: "submarine", name: "Submarine", size: 3 },
  { id: "destroyer", name: "Destroyer", size: 2 },
];

export type Player = "alice" | "bob" | "player";

export interface ChainCell {
  salt: Uint8Array;
  isOccupied: boolean;
}

export type OnchainPhase =
  | "menu"
  | "creating"
  | "waiting_opponent"
  | "setup"
  | "waiting_commit"
  | "battle"
  | "revealing"
  | "finished";
