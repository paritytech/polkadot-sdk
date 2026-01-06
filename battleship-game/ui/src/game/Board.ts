import {
  Cell,
  Position,
  PlacedShip,
  ShipDefinition,
  Orientation,
  GRID_SIZE,
  SHIP_DEFINITIONS,
} from "../types/index.ts";
import {
  createPlacedShip,
  getShipCells,
  canPlaceShip,
  hitShip,
  isShipSunk,
  getRandomPlacement,
} from "./Ship.ts";

export class Board {
  private grid: Cell[][];
  private ships: PlacedShip[] = [];

  constructor() {
    this.grid = this.createEmptyGrid();
  }

  private createEmptyGrid(): Cell[][] {
    const grid: Cell[][] = [];
    for (let y = 0; y < GRID_SIZE; y++) {
      const row: Cell[] = [];
      for (let x = 0; x < GRID_SIZE; x++) {
        row.push({ state: "empty", shipId: null });
      }
      grid.push(row);
    }
    return grid;
  }

  getCell(pos: Position): Cell | null {
    if (pos.x < 0 || pos.x >= GRID_SIZE || pos.y < 0 || pos.y >= GRID_SIZE) {
      return null;
    }
    return this.grid[pos.y][pos.x];
  }

  getShips(): PlacedShip[] {
    return this.ships;
  }

  canPlace(
    definition: ShipDefinition,
    position: Position,
    orientation: Orientation
  ): boolean {
    return canPlaceShip(definition, position, orientation, this.ships);
  }

  placeShip(
    definition: ShipDefinition,
    position: Position,
    orientation: Orientation
  ): boolean {
    if (!this.canPlace(definition, position, orientation)) {
      return false;
    }

    const ship = createPlacedShip(definition, position, orientation);
    this.ships.push(ship);

    const cells = getShipCells(ship);
    for (const cell of cells) {
      this.grid[cell.y][cell.x] = { state: "ship", shipId: definition.id };
    }

    return true;
  }

  removeShip(shipId: string): void {
    const shipIndex = this.ships.findIndex((s) => s.definition.id === shipId);
    if (shipIndex === -1) return;

    const ship = this.ships[shipIndex];
    const cells = getShipCells(ship);

    for (const cell of cells) {
      this.grid[cell.y][cell.x] = { state: "empty", shipId: null };
    }

    this.ships.splice(shipIndex, 1);
  }

  receiveAttack(pos: Position): "hit" | "miss" | "already_attacked" | "invalid" {
    const cell = this.getCell(pos);
    if (!cell) return "invalid";

    if (cell.state === "hit" || cell.state === "miss") {
      return "already_attacked";
    }

    if (cell.state === "ship") {
      this.grid[pos.y][pos.x].state = "hit";

      const ship = this.ships.find((s) => s.definition.id === cell.shipId);
      if (ship) {
        const cells = getShipCells(ship);
        const cellIndex = cells.findIndex(
          (c) => c.x === pos.x && c.y === pos.y
        );
        if (cellIndex !== -1) {
          hitShip(ship, cellIndex);
        }
      }

      return "hit";
    }

    this.grid[pos.y][pos.x].state = "miss";
    return "miss";
  }

  // Mark a cell as hit or miss (for opponent board where we don't know ship positions)
  markAttackResult(pos: Position, isHit: boolean): void {
    if (pos.x < 0 || pos.x >= GRID_SIZE || pos.y < 0 || pos.y >= GRID_SIZE) {
      return;
    }
    this.grid[pos.y][pos.x].state = isHit ? "hit" : "miss";
  }

  isShipSunk(shipId: string): boolean {
    const ship = this.ships.find((s) => s.definition.id === shipId);
    return ship ? isShipSunk(ship) : false;
  }

  allShipsSunk(): boolean {
    return this.ships.every((ship) => isShipSunk(ship));
  }

  allShipsPlaced(): boolean {
    return this.ships.length === SHIP_DEFINITIONS.length;
  }

  placeShipsRandomly(): void {
    this.reset();

    for (const definition of SHIP_DEFINITIONS) {
      const placement = getRandomPlacement(definition, this.ships);
      if (placement) {
        this.placeShip(definition, placement.position, placement.orientation);
      }
    }
  }

  reset(): void {
    this.grid = this.createEmptyGrid();
    this.ships = [];
  }

  getGrid(): Cell[][] {
    return this.grid;
  }
}
