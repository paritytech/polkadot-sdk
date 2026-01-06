import { Position } from "../types/index.ts";
import { Renderer } from "../render/Renderer.ts";

export type InputCallback = (pos: Position) => void;

export class InputHandler {
  private renderer: Renderer;
  private hoverPos: Position | null = null;
  private onClickCallback: InputCallback | null = null;
  private onHoverCallback: ((pos: Position | null) => void) | null = null;
  private onKeyCallback: ((key: string) => void) | null = null;

  constructor(
    canvas: HTMLCanvasElement,
    renderer: Renderer
  ) {
    this.renderer = renderer;
    this.setupListeners(canvas);
  }

  private setupListeners(canvas: HTMLCanvasElement): void {
    canvas.addEventListener("mousemove", (e) => {
      const rect = canvas.getBoundingClientRect();
      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;
      const gridPos = this.renderer.screenToGrid(x, y);

      if (
        gridPos?.x !== this.hoverPos?.x ||
        gridPos?.y !== this.hoverPos?.y
      ) {
        this.hoverPos = gridPos;
        this.onHoverCallback?.(gridPos);
      }
    });

    canvas.addEventListener("mouseleave", () => {
      this.hoverPos = null;
      this.onHoverCallback?.(null);
    });

    canvas.addEventListener("click", (e) => {
      const rect = canvas.getBoundingClientRect();
      const x = e.clientX - rect.left;
      const y = e.clientY - rect.top;
      const gridPos = this.renderer.screenToGrid(x, y);

      if (gridPos && this.onClickCallback) {
        this.onClickCallback(gridPos);
      }
    });

    document.addEventListener("keydown", (e) => {
      this.onKeyCallback?.(e.key);
    });
  }

  onClick(callback: InputCallback): void {
    this.onClickCallback = callback;
  }

  onHover(callback: (pos: Position | null) => void): void {
    this.onHoverCallback = callback;
  }

  onKey(callback: (key: string) => void): void {
    this.onKeyCallback = callback;
  }

  getHoverPos(): Position | null {
    return this.hoverPos;
  }
}
