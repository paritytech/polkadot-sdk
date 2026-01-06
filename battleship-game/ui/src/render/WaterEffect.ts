export class WaterEffect {
  private time = 0;

  update(deltaTime: number): void {
    this.time += deltaTime * 0.001;
  }

  render(ctx: CanvasRenderingContext2D, width: number, height: number): void {
    // Deep ocean gradient
    const gradient = ctx.createLinearGradient(0, 0, width * 0.5, height);
    gradient.addColorStop(0, "#0a2463");
    gradient.addColorStop(0.3, "#1e5799");
    gradient.addColorStop(0.6, "#2989d8");
    gradient.addColorStop(1, "#1e3c72");

    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, width, height);

    // Draw animated wave layers
    this.drawWaveLayers(ctx, width, height);

    // Add caustic light effects
    this.drawCaustics(ctx, width, height);

    // Surface shimmer
    this.drawShimmer(ctx, width, height);
  }

  private drawWaveLayers(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number
  ): void {
    const waveLayers = [
      { yBase: 0.15, amplitude: 4, frequency: 0.008, speed: 0.6, alpha: 0.08 },
      { yBase: 0.3, amplitude: 6, frequency: 0.012, speed: 0.8, alpha: 0.1 },
      { yBase: 0.5, amplitude: 5, frequency: 0.015, speed: 1.0, alpha: 0.12 },
      { yBase: 0.7, amplitude: 4, frequency: 0.02, speed: 1.2, alpha: 0.1 },
      { yBase: 0.85, amplitude: 3, frequency: 0.025, speed: 1.4, alpha: 0.08 },
    ];

    for (const layer of waveLayers) {
      this.drawWaveLayer(ctx, width, height, layer);
    }
  }

  private drawWaveLayer(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number,
    config: {
      yBase: number;
      amplitude: number;
      frequency: number;
      speed: number;
      alpha: number;
    }
  ): void {
    const baseY = height * config.yBase;

    ctx.beginPath();
    ctx.moveTo(0, height);
    ctx.lineTo(0, baseY);

    for (let x = 0; x <= width; x += 3) {
      const wave1 = Math.sin(x * config.frequency + this.time * config.speed);
      const wave2 = Math.sin(
        x * config.frequency * 0.6 + this.time * config.speed * 0.8 + 1.5
      );
      const wave3 = Math.sin(
        x * config.frequency * 1.4 + this.time * config.speed * 1.2 + 3.0
      );

      const y =
        baseY +
        wave1 * config.amplitude +
        wave2 * config.amplitude * 0.5 +
        wave3 * config.amplitude * 0.3;

      ctx.lineTo(x, y);
    }

    ctx.lineTo(width, height);
    ctx.closePath();

    const gradient = ctx.createLinearGradient(0, baseY - 20, 0, baseY + 50);
    gradient.addColorStop(0, `rgba(255, 255, 255, ${config.alpha})`);
    gradient.addColorStop(0.5, `rgba(200, 230, 255, ${config.alpha * 0.5})`);
    gradient.addColorStop(1, `rgba(255, 255, 255, 0)`);

    ctx.fillStyle = gradient;
    ctx.fill();
  }

  private drawCaustics(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number
  ): void {
    ctx.save();

    const cellSize = 60;
    const cols = Math.ceil(width / cellSize) + 1;
    const rows = Math.ceil(height / cellSize) + 1;

    for (let row = 0; row < rows; row++) {
      for (let col = 0; col < cols; col++) {
        const baseX = col * cellSize;
        const baseY = row * cellSize;

        const offsetX =
          Math.sin(this.time * 0.5 + col * 0.8 + row * 0.3) * 10;
        const offsetY =
          Math.cos(this.time * 0.4 + col * 0.5 + row * 0.7) * 8;

        const x = baseX + offsetX;
        const y = baseY + offsetY;

        const brightness =
          0.03 +
          0.02 *
            Math.sin(this.time * 0.8 + col * 1.2 + row * 0.9) *
            Math.cos(this.time * 0.6 + col * 0.7 + row * 1.1);

        if (brightness > 0.02) {
          ctx.fillStyle = `rgba(150, 200, 255, ${brightness})`;
          ctx.beginPath();

          // Draw organic caustic shape
          const points = 6;
          for (let i = 0; i <= points; i++) {
            const angle = (i / points) * Math.PI * 2;
            const radius =
              15 +
              5 * Math.sin(angle * 3 + this.time) +
              3 * Math.cos(angle * 5 - this.time * 0.7);
            const px = x + Math.cos(angle) * radius;
            const py = y + Math.sin(angle) * radius * 0.6;

            if (i === 0) {
              ctx.moveTo(px, py);
            } else {
              ctx.lineTo(px, py);
            }
          }
          ctx.closePath();
          ctx.fill();
        }
      }
    }

    ctx.restore();
  }

  private drawShimmer(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number
  ): void {
    const shimmerCount = 40;

    ctx.save();
    for (let i = 0; i < shimmerCount; i++) {
      const seed = i * 7654.321;
      const x = ((seed * 0.31) % 1) * width;
      const y = ((seed * 0.57) % 1) * height;

      const flickerSpeed = 1.5 + (seed % 2);
      const alpha = 0.15 + 0.15 * Math.sin(this.time * flickerSpeed + seed);

      if (alpha > 0.1) {
        // Star-like shimmer
        ctx.fillStyle = `rgba(255, 255, 255, ${alpha})`;

        ctx.beginPath();
        const size = 1.5 + Math.sin(this.time * 2 + seed) * 0.5;

        // 4-point star
        ctx.moveTo(x, y - size * 2);
        ctx.lineTo(x + size * 0.5, y - size * 0.5);
        ctx.lineTo(x + size * 2, y);
        ctx.lineTo(x + size * 0.5, y + size * 0.5);
        ctx.lineTo(x, y + size * 2);
        ctx.lineTo(x - size * 0.5, y + size * 0.5);
        ctx.lineTo(x - size * 2, y);
        ctx.lineTo(x - size * 0.5, y - size * 0.5);
        ctx.closePath();
        ctx.fill();
      }
    }
    ctx.restore();
  }
}
