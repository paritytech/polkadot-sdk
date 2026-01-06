import {
  GRID_SIZE,
  TILE_WIDTH,
  TILE_HEIGHT,
  Cell,
  Position,
} from "../types/index.ts";
import { gridToScreen, drawIsoDiamond, screenToGrid } from "./IsoUtils.ts";

export class GridRenderer {
  render(
    ctx: CanvasRenderingContext2D,
    grid: Cell[][],
    showShips: boolean,
    hoverCell: Position | null = null
  ): void {
    this.drawGrid(ctx);
    this.drawCells(ctx, grid, showShips);
    this.drawLabels(ctx);

    if (hoverCell) {
      this.drawHover(ctx, hoverCell);
    }
  }

  private drawGrid(ctx: CanvasRenderingContext2D): void {
    ctx.strokeStyle = "rgba(255, 255, 255, 0.3)";
    ctx.lineWidth = 1;

    for (let y = 0; y < GRID_SIZE; y++) {
      for (let x = 0; x < GRID_SIZE; x++) {
        drawIsoDiamond(ctx, x, y);
        ctx.stroke();
      }
    }
  }

  private drawCells(
    ctx: CanvasRenderingContext2D,
    grid: Cell[][],
    _showShips: boolean
  ): void {
    for (let y = 0; y < GRID_SIZE; y++) {
      for (let x = 0; x < GRID_SIZE; x++) {
        const cell = grid[y][x];

        if (cell.state === "hit") {
          this.drawHit(ctx, x, y);
        } else if (cell.state === "miss") {
          this.drawMiss(ctx, x, y);
        }
      }
    }
  }

  private drawHit(ctx: CanvasRenderingContext2D, gridX: number, gridY: number): void {
    const pos = gridToScreen(gridX, gridY);
    const centerX = pos.x;
    const centerY = pos.y + TILE_HEIGHT / 2;

    drawIsoDiamond(ctx, gridX, gridY);
    ctx.fillStyle = "rgba(200, 50, 50, 0.6)";
    ctx.fill();

    // X mark
    const size = TILE_HEIGHT * 0.4;
    ctx.strokeStyle = "#ff3333";
    ctx.lineWidth = 3;
    ctx.lineCap = "round";

    ctx.beginPath();
    ctx.moveTo(centerX - size, centerY - size * 0.5);
    ctx.lineTo(centerX + size, centerY + size * 0.5);
    ctx.stroke();

    ctx.beginPath();
    ctx.moveTo(centerX + size, centerY - size * 0.5);
    ctx.lineTo(centerX - size, centerY + size * 0.5);
    ctx.stroke();
  }

  private drawMiss(ctx: CanvasRenderingContext2D, gridX: number, gridY: number): void {
    const pos = gridToScreen(gridX, gridY);
    const centerX = pos.x;
    const centerY = pos.y + TILE_HEIGHT / 2;

    ctx.fillStyle = "rgba(255, 255, 255, 0.3)";
    ctx.beginPath();
    ctx.ellipse(centerX, centerY, TILE_WIDTH * 0.2, TILE_HEIGHT * 0.2, 0, 0, Math.PI * 2);
    ctx.fill();

    ctx.strokeStyle = "rgba(255, 255, 255, 0.5)";
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.ellipse(centerX, centerY, TILE_WIDTH * 0.3, TILE_HEIGHT * 0.3, 0, 0, Math.PI * 2);
    ctx.stroke();
  }

  private drawHover(ctx: CanvasRenderingContext2D, pos: Position): void {
    drawIsoDiamond(ctx, pos.x, pos.y);
    ctx.fillStyle = "rgba(255, 255, 255, 0.2)";
    ctx.fill();
    ctx.strokeStyle = "rgba(255, 255, 255, 0.6)";
    ctx.lineWidth = 2;
    ctx.stroke();
  }

  private drawLabels(ctx: CanvasRenderingContext2D): void {
    ctx.fillStyle = "rgba(255, 255, 255, 0.7)";
    ctx.font = "bold 12px sans-serif";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";

    // Column labels (A-J) along top-right edge
    for (let i = 0; i < GRID_SIZE; i++) {
      const pos = gridToScreen(i, -1);
      ctx.fillText(String.fromCharCode(65 + i), pos.x, pos.y + TILE_HEIGHT / 2);
    }

    // Row labels (1-10) along top-left edge
    for (let i = 0; i < GRID_SIZE; i++) {
      const pos = gridToScreen(-1, i);
      ctx.fillText(String(i + 1), pos.x, pos.y + TILE_HEIGHT / 2);
    }
  }

  screenToGrid(screenX: number, screenY: number): Position | null {
    return screenToGrid(screenX, screenY);
  }
}
