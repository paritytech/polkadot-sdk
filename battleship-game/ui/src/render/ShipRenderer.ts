import {
  PlacedShip,
  ShipDefinition,
  Position,
  Orientation,
  TILE_WIDTH,
  TILE_HEIGHT,
} from "../types/index.ts";
import { getShipCells, isShipSunk } from "../game/Ship.ts";
import { gridToScreen } from "./IsoUtils.ts";

interface ShipColors {
  hull: string;
  hullDark: string;
  hullLight: string;
  deck: string;
  accent: string;
}

const SHIP_COLORS: Record<string, ShipColors> = {
  carrier: { hull: "#5a6578", hullDark: "#3d4555", hullLight: "#7a8598", deck: "#4a5568", accent: "#cc3333" },
  battleship: { hull: "#6b6b7a", hullDark: "#4a4a55", hullLight: "#8a8a9a", deck: "#5a5a6a", accent: "#ddcc44" },
  cruiser: { hull: "#5a6a7a", hullDark: "#404a55", hullLight: "#7a8a9a", deck: "#4a5a6a", accent: "#44bb66" },
  submarine: { hull: "#3a4a3a", hullDark: "#2a352a", hullLight: "#4a5f4a", deck: "#3a4a3a", accent: "#ee6666" },
  destroyer: { hull: "#6a6055", hullDark: "#4a4540", hullLight: "#8a8070", deck: "#5a5550", accent: "#66aadd" },
};

const SUNK_COLORS: ShipColors = {
  hull: "#2a2a2a", hullDark: "#1a1a1a", hullLight: "#3a3a3a", deck: "#353535", accent: "#444444"
};

export class ShipRenderer {
  renderPlacedShips(
    ctx: CanvasRenderingContext2D,
    ships: PlacedShip[],
    showSunk: boolean = false
  ): void {
    const sortedShips = [...ships].sort((a, b) => {
      return (a.position.x + a.position.y) - (b.position.x + b.position.y);
    });

    for (const ship of sortedShips) {
      if (showSunk && !isShipSunk(ship)) continue;
      this.renderShip(ctx, ship);
    }
  }

  renderShip(ctx: CanvasRenderingContext2D, ship: PlacedShip): void {
    const sunk = isShipSunk(ship);
    const colors = sunk ? SUNK_COLORS : SHIP_COLORS[ship.definition.id];
    const cells = getShipCells(ship);

    // Draw cell highlights first (so ship renders on top)
    this.drawCellHighlights(ctx, cells, colors, sunk);

    // Get screen positions for first and last cells
    const firstCell = this.getTileCenter(cells[0]);
    const lastCell = this.getTileCenter(cells[cells.length - 1]);

    // Calculate direction along ship (stern to bow)
    const dx = lastCell.x - firstCell.x;
    const dy = lastCell.y - firstCell.y;
    const len = Math.sqrt(dx * dx + dy * dy);

    // Unit vectors: dir = along ship, perp = across ship
    const dirX = len > 0 ? dx / len : 1;
    const dirY = len > 0 ? dy / len : 0;
    const perpX = -dirY;
    const perpY = dirX;

    // Ship dimensions
    const shipLen = len + 20;
    const shipWidth = 12 + ship.definition.size * 2;

    // Ship center
    const cx = (firstCell.x + lastCell.x) / 2;
    const cy = (firstCell.y + lastCell.y) / 2;

    // Draw the ship hull following the cell direction
    this.drawShipHull(ctx, cx, cy, dirX, dirY, perpX, perpY, shipLen, shipWidth, colors);

    // Draw superstructure
    this.drawShipDetails(ctx, cx, cy, dirX, dirY, perpX, perpY, shipLen, shipWidth, colors, ship.definition.id, sunk);

    // Draw damage on hit cells
    for (let i = 0; i < cells.length; i++) {
      if (ship.hits.has(i)) {
        this.drawDamage(ctx, cells[i]);
      }
    }
  }

  private getTileCenter(gridPos: Position): Position {
    const screen = gridToScreen(gridPos.x, gridPos.y);
    return { x: screen.x, y: screen.y + TILE_HEIGHT / 2 };
  }

  private drawCellHighlights(ctx: CanvasRenderingContext2D, cells: Position[], colors: ShipColors, sunk: boolean): void {
    ctx.save();
    for (const cell of cells) {
      const screen = gridToScreen(cell.x, cell.y);
      ctx.beginPath();
      ctx.moveTo(screen.x, screen.y);
      ctx.lineTo(screen.x + TILE_WIDTH / 2, screen.y + TILE_HEIGHT / 2);
      ctx.lineTo(screen.x, screen.y + TILE_HEIGHT);
      ctx.lineTo(screen.x - TILE_WIDTH / 2, screen.y + TILE_HEIGHT / 2);
      ctx.closePath();
      ctx.fillStyle = sunk ? "rgba(60, 30, 30, 0.4)" : `${colors.hull}66`;
      ctx.fill();
      ctx.strokeStyle = sunk ? "rgba(100, 50, 50, 0.5)" : `${colors.hullLight}88`;
      ctx.lineWidth = 1;
      ctx.stroke();
    }
    ctx.restore();
  }

  private drawShipHull(
    ctx: CanvasRenderingContext2D,
    cx: number, cy: number,
    dirX: number, dirY: number,
    perpX: number, perpY: number,
    length: number, width: number,
    colors: ShipColors
  ): void {
    const halfLen = length / 2;
    const halfWidth = width / 2;
    const bowTaper = 0.3;
    const depth = 5;

    const sternX = cx - dirX * halfLen;
    const sternY = cy - dirY * halfLen;
    const bowX = cx + dirX * halfLen;
    const bowY = cy + dirY * halfLen;

    // Shadow
    ctx.fillStyle = "rgba(0,20,40,0.3)";
    ctx.beginPath();
    ctx.moveTo(sternX + perpX * halfWidth, sternY + perpY * halfWidth + depth);
    ctx.lineTo(sternX - perpX * halfWidth, sternY - perpY * halfWidth + depth);
    ctx.quadraticCurveTo(
      bowX - perpX * halfWidth * bowTaper, bowY - perpY * halfWidth * bowTaper + depth,
      bowX, bowY + depth
    );
    ctx.quadraticCurveTo(
      bowX + perpX * halfWidth * bowTaper, bowY + perpY * halfWidth * bowTaper + depth,
      sternX + perpX * halfWidth, sternY + perpY * halfWidth + depth
    );
    ctx.fill();

    // Hull side (3D depth)
    ctx.fillStyle = colors.hullDark;
    ctx.beginPath();
    ctx.moveTo(sternX - perpX * halfWidth, sternY - perpY * halfWidth);
    ctx.lineTo(sternX - perpX * halfWidth, sternY - perpY * halfWidth + depth);
    ctx.quadraticCurveTo(
      bowX - perpX * halfWidth * bowTaper, bowY - perpY * halfWidth * bowTaper + depth,
      bowX, bowY + depth
    );
    ctx.lineTo(bowX, bowY);
    ctx.quadraticCurveTo(
      bowX - perpX * halfWidth * bowTaper, bowY - perpY * halfWidth * bowTaper,
      sternX - perpX * halfWidth, sternY - perpY * halfWidth
    );
    ctx.fill();

    // Hull top
    ctx.fillStyle = colors.hull;
    ctx.beginPath();
    ctx.moveTo(sternX + perpX * halfWidth, sternY + perpY * halfWidth);
    ctx.lineTo(sternX - perpX * halfWidth, sternY - perpY * halfWidth);
    ctx.quadraticCurveTo(
      bowX - perpX * halfWidth * bowTaper, bowY - perpY * halfWidth * bowTaper,
      bowX, bowY
    );
    ctx.quadraticCurveTo(
      bowX + perpX * halfWidth * bowTaper, bowY + perpY * halfWidth * bowTaper,
      sternX + perpX * halfWidth, sternY + perpY * halfWidth
    );
    ctx.fill();

    // Deck
    ctx.fillStyle = colors.deck;
    ctx.beginPath();
    const deckInset = 0.7;
    const deckLen = halfLen * 0.8;
    const ds = { x: cx - dirX * deckLen, y: cy - dirY * deckLen };
    const db = { x: cx + dirX * deckLen * 0.6, y: cy + dirY * deckLen * 0.6 };
    ctx.moveTo(ds.x + perpX * halfWidth * deckInset, ds.y + perpY * halfWidth * deckInset);
    ctx.lineTo(ds.x - perpX * halfWidth * deckInset, ds.y - perpY * halfWidth * deckInset);
    ctx.quadraticCurveTo(db.x, db.y, ds.x + perpX * halfWidth * deckInset, ds.y + perpY * halfWidth * deckInset);
    ctx.fill();

    // Hull highlight
    ctx.strokeStyle = colors.hullLight;
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.moveTo(sternX + perpX * halfWidth, sternY + perpY * halfWidth);
    ctx.quadraticCurveTo(
      bowX + perpX * halfWidth * bowTaper, bowY + perpY * halfWidth * bowTaper,
      bowX, bowY
    );
    ctx.stroke();
  }

  private drawShipDetails(
    ctx: CanvasRenderingContext2D,
    cx: number, cy: number,
    dirX: number, dirY: number,
    _perpX: number, _perpY: number,
    length: number, _width: number,
    colors: ShipColors, shipType: string, sunk: boolean
  ): void {
    const halfLen = length / 2;

    // Bridge position
    const bridgeX = cx - dirX * halfLen * 0.1;
    const bridgeY = cy - dirY * halfLen * 0.1;

    // Bridge
    ctx.fillStyle = colors.hullDark;
    ctx.beginPath();
    ctx.ellipse(bridgeX, bridgeY - 6, 5, 3, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillRect(bridgeX - 4, bridgeY - 12, 8, 7);

    ctx.fillStyle = colors.hull;
    ctx.fillRect(bridgeX - 5, bridgeY - 14, 10, 3);

    if (!sunk) {
      // Mast
      ctx.strokeStyle = colors.accent;
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      ctx.moveTo(bridgeX, bridgeY - 14);
      ctx.lineTo(bridgeX, bridgeY - 20);
      ctx.stroke();

      // Gun turrets for combat ships
      if (shipType === "battleship" || shipType === "cruiser" || shipType === "destroyer") {
        const fwdX = cx + dirX * halfLen * 0.4;
        const fwdY = cy + dirY * halfLen * 0.4;
        this.drawTurretAt(ctx, fwdX, fwdY, dirX, dirY, colors, shipType === "battleship");
      }

      if (shipType === "battleship") {
        const rearX = cx - dirX * halfLen * 0.45;
        const rearY = cy - dirY * halfLen * 0.45;
        this.drawTurretAt(ctx, rearX, rearY, dirX, dirY, colors, true);
      }

      // Submarine conning tower
      if (shipType === "submarine") {
        ctx.fillStyle = colors.hullDark;
        ctx.beginPath();
        ctx.ellipse(bridgeX, bridgeY - 2, 4, 2.5, 0, 0, Math.PI * 2);
        ctx.fill();
      }
    }
  }

  private drawTurretAt(
    ctx: CanvasRenderingContext2D,
    x: number, y: number,
    dirX: number, dirY: number,
    colors: ShipColors, large: boolean
  ): void {
    const size = large ? 4 : 3;

    ctx.fillStyle = colors.hullDark;
    ctx.beginPath();
    ctx.ellipse(x, y - 1, size, size * 0.5, 0, 0, Math.PI * 2);
    ctx.fill();

    ctx.fillStyle = colors.hull;
    ctx.beginPath();
    ctx.ellipse(x, y - 2.5, size - 0.5, (size - 0.5) * 0.5, 0, 0, Math.PI * 2);
    ctx.fill();

    // Barrel pointing toward bow
    ctx.fillStyle = colors.accent;
    const barrelLen = large ? 10 : 7;
    ctx.save();
    ctx.translate(x, y - 2);
    const barrelAngle = Math.atan2(dirY, dirX);
    ctx.rotate(barrelAngle);
    ctx.fillRect(0, -1, barrelLen, 2);
    if (large) {
      ctx.fillRect(0, -2.5, barrelLen - 1, 1.5);
    }
    ctx.restore();
  }

  private drawDamage(ctx: CanvasRenderingContext2D, gridPos: Position): void {
    const center = this.getTileCenter(gridPos);

    const gradient = ctx.createRadialGradient(center.x, center.y - 5, 0, center.x, center.y - 5, 14);
    gradient.addColorStop(0, "rgba(255, 220, 80, 0.95)");
    gradient.addColorStop(0.3, "rgba(255, 120, 30, 0.85)");
    gradient.addColorStop(0.6, "rgba(200, 60, 20, 0.6)");
    gradient.addColorStop(1, "rgba(80, 30, 20, 0)");

    ctx.fillStyle = gradient;
    ctx.beginPath();
    ctx.arc(center.x, center.y - 5, 14, 0, Math.PI * 2);
    ctx.fill();

    ctx.fillStyle = "rgba(40, 40, 40, 0.7)";
    ctx.beginPath();
    ctx.arc(center.x - 3, center.y - 16, 6, 0, Math.PI * 2);
    ctx.arc(center.x + 2, center.y - 24, 5, 0, Math.PI * 2);
    ctx.fill();
  }

  renderPreview(
    ctx: CanvasRenderingContext2D,
    definition: ShipDefinition,
    position: Position,
    orientation: Orientation,
    valid: boolean
  ): void {
    ctx.save();
    ctx.globalAlpha = 0.6;

    for (let i = 0; i < definition.size; i++) {
      const cellX = orientation === "horizontal" ? position.x + i : position.x;
      const cellY = orientation === "vertical" ? position.y + i : position.y;
      const screen = gridToScreen(cellX, cellY);

      ctx.beginPath();
      ctx.moveTo(screen.x, screen.y);
      ctx.lineTo(screen.x + TILE_WIDTH / 2, screen.y + TILE_HEIGHT / 2);
      ctx.lineTo(screen.x, screen.y + TILE_HEIGHT);
      ctx.lineTo(screen.x - TILE_WIDTH / 2, screen.y + TILE_HEIGHT / 2);
      ctx.closePath();

      ctx.fillStyle = valid ? "rgba(100, 200, 100, 0.5)" : "rgba(200, 100, 100, 0.5)";
      ctx.fill();
      ctx.strokeStyle = valid ? "rgba(100, 255, 100, 0.8)" : "rgba(255, 100, 100, 0.8)";
      ctx.lineWidth = 2;
      ctx.stroke();
    }

    ctx.restore();
  }
}
