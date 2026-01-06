import { Board } from "../game/Board.ts";
import { WaterEffect } from "./WaterEffect.ts";
import { GridRenderer } from "./GridRenderer.ts";
import { ShipRenderer } from "./ShipRenderer.ts";
import {
  Position,
  ShipDefinition,
  Orientation,
} from "../types/index.ts";
import { screenToGrid } from "./IsoUtils.ts";

export class Renderer {
  private waterEffect: WaterEffect;
  private gridRenderer: GridRenderer;
  private shipRenderer: ShipRenderer;

  constructor() {
    this.waterEffect = new WaterEffect();
    this.gridRenderer = new GridRenderer();
    this.shipRenderer = new ShipRenderer();
  }

  update(deltaTime: number): void {
    this.waterEffect.update(deltaTime);
  }

  renderPlayerBoard(
    ctx: CanvasRenderingContext2D,
    board: Board,
    hoverCell: Position | null,
    placementPreview: {
      definition: ShipDefinition;
      position: Position;
      orientation: Orientation;
      valid: boolean;
    } | null
  ): void {
    const width = ctx.canvas.width;
    const height = ctx.canvas.height;

    ctx.clearRect(0, 0, width, height);
    this.waterEffect.render(ctx, width, height);
    this.gridRenderer.render(ctx, board.getGrid(), true, hoverCell);
    this.shipRenderer.renderPlacedShips(ctx, board.getShips());

    if (placementPreview) {
      this.shipRenderer.renderPreview(
        ctx,
        placementPreview.definition,
        placementPreview.position,
        placementPreview.orientation,
        placementPreview.valid
      );
    }
  }

  renderEnemyBoard(
    ctx: CanvasRenderingContext2D,
    board: Board,
    hoverCell: Position | null,
    canAttack: boolean
  ): void {
    const width = ctx.canvas.width;
    const height = ctx.canvas.height;

    ctx.clearRect(0, 0, width, height);
    this.waterEffect.render(ctx, width, height);
    this.gridRenderer.render(
      ctx,
      board.getGrid(),
      false,
      canAttack ? hoverCell : null
    );

    // Only show sunk ships on enemy board
    this.shipRenderer.renderPlacedShips(ctx, board.getShips(), true);
  }

  screenToGrid(screenX: number, screenY: number): Position | null {
    return screenToGrid(screenX, screenY);
  }
}
