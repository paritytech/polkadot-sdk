import {
  ShipDefinition,
  Position,
  Orientation,
  PlacedShip,
  GRID_SIZE,
} from "../types/index.ts";

export function createPlacedShip(
  definition: ShipDefinition,
  position: Position,
  orientation: Orientation
): PlacedShip {
  return {
    definition,
    position,
    orientation,
    hits: new Set(),
  };
}

export function getShipCells(ship: PlacedShip): Position[] {
  const cells: Position[] = [];
  for (let i = 0; i < ship.definition.size; i++) {
    if (ship.orientation === "horizontal") {
      cells.push({ x: ship.position.x + i, y: ship.position.y });
    } else {
      cells.push({ x: ship.position.x, y: ship.position.y + i });
    }
  }
  return cells;
}

export function isShipSunk(ship: PlacedShip): boolean {
  return ship.hits.size >= ship.definition.size;
}

export function hitShip(ship: PlacedShip, cellIndex: number): void {
  ship.hits.add(cellIndex);
}

export function canPlaceShip(
  definition: ShipDefinition,
  position: Position,
  orientation: Orientation,
  existingShips: PlacedShip[]
): boolean {
  const tempShip = createPlacedShip(definition, position, orientation);
  const cells = getShipCells(tempShip);

  // Check bounds
  for (const cell of cells) {
    if (cell.x < 0 || cell.x >= GRID_SIZE || cell.y < 0 || cell.y >= GRID_SIZE) {
      return false;
    }
  }

  // Check collision with existing ships
  for (const existingShip of existingShips) {
    const existingCells = getShipCells(existingShip);
    for (const cell of cells) {
      for (const existingCell of existingCells) {
        if (cell.x === existingCell.x && cell.y === existingCell.y) {
          return false;
        }
      }
    }
  }

  return true;
}

export function getRandomPlacement(
  definition: ShipDefinition,
  existingShips: PlacedShip[]
): { position: Position; orientation: Orientation } | null {
  const orientations: Orientation[] = ["horizontal", "vertical"];
  const maxAttempts = 100;

  for (let attempt = 0; attempt < maxAttempts; attempt++) {
    const orientation = orientations[Math.floor(Math.random() * 2)];
    const maxX =
      orientation === "horizontal" ? GRID_SIZE - definition.size : GRID_SIZE;
    const maxY =
      orientation === "vertical" ? GRID_SIZE - definition.size : GRID_SIZE;

    const position: Position = {
      x: Math.floor(Math.random() * maxX),
      y: Math.floor(Math.random() * maxY),
    };

    if (canPlaceShip(definition, position, orientation, existingShips)) {
      return { position, orientation };
    }
  }

  return null;
}
