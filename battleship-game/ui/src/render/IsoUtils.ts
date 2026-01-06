import {
  Position,
  TILE_WIDTH,
  TILE_HEIGHT,
  BOARD_OFFSET_X,
  BOARD_OFFSET_Y,
  GRID_SIZE,
} from "../types/index.ts";

export function gridToScreen(gridX: number, gridY: number): Position {
  const screenX = BOARD_OFFSET_X + (gridX - gridY) * (TILE_WIDTH / 2);
  const screenY = BOARD_OFFSET_Y + (gridX + gridY) * (TILE_HEIGHT / 2);
  return { x: screenX, y: screenY };
}

export function screenToGrid(screenX: number, screenY: number): Position | null {
  const relX = screenX - BOARD_OFFSET_X;
  const relY = screenY - BOARD_OFFSET_Y;

  const gridX = (relX / (TILE_WIDTH / 2) + relY / (TILE_HEIGHT / 2)) / 2;
  const gridY = (relY / (TILE_HEIGHT / 2) - relX / (TILE_WIDTH / 2)) / 2;

  const roundedX = Math.floor(gridX);
  const roundedY = Math.floor(gridY);

  if (
    roundedX >= 0 &&
    roundedX < GRID_SIZE &&
    roundedY >= 0 &&
    roundedY < GRID_SIZE
  ) {
    return { x: roundedX, y: roundedY };
  }

  return null;
}

export function getTileCorners(gridX: number, gridY: number): Position[] {
  const center = gridToScreen(gridX, gridY);
  return [
    { x: center.x, y: center.y },
    { x: center.x + TILE_WIDTH / 2, y: center.y + TILE_HEIGHT / 2 },
    { x: center.x, y: center.y + TILE_HEIGHT },
    { x: center.x - TILE_WIDTH / 2, y: center.y + TILE_HEIGHT / 2 },
  ];
}

export function drawIsoDiamond(
  ctx: CanvasRenderingContext2D,
  gridX: number,
  gridY: number
): void {
  const corners = getTileCorners(gridX, gridY);
  ctx.beginPath();
  ctx.moveTo(corners[0].x, corners[0].y);
  ctx.lineTo(corners[1].x, corners[1].y);
  ctx.lineTo(corners[2].x, corners[2].y);
  ctx.lineTo(corners[3].x, corners[3].y);
  ctx.closePath();
}
