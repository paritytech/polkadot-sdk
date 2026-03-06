export interface Position {
  x: number;
  y: number;
}

export interface ChainCell {
  salt: Uint8Array;
  isOccupied: boolean;
}

export interface ShipDefinition {
  id: string;
  name: string;
  size: number;
}

export const SHIP_DEFINITIONS: ShipDefinition[] = [
  { id: "carrier", name: "Carrier", size: 5 },
  { id: "battleship", name: "Battleship", size: 4 },
  { id: "cruiser", name: "Cruiser", size: 3 },
  { id: "submarine", name: "Submarine", size: 3 },
  { id: "destroyer", name: "Destroyer", size: 2 },
];

export const GRID_SIZE = 10;
