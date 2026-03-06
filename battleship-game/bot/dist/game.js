import { SHIP_DEFINITIONS, GRID_SIZE } from "./types.js";
import { coordToIndex } from "./merkle.js";
export function generateRandomShipPlacement() {
    const maxAttempts = 1000;
    for (let attempt = 0; attempt < maxAttempts; attempt++) {
        const occupied = new Set();
        let success = true;
        for (const ship of SHIP_DEFINITIONS) {
            const placed = tryPlaceShip(ship, occupied);
            if (!placed) {
                success = false;
                break;
            }
        }
        if (success && occupied.size === 17) {
            console.log(`✓ Successfully placed all ships (attempt ${attempt + 1})`);
            return occupied;
        }
    }
    throw new Error("Failed to generate valid ship placement after max attempts");
}
function tryPlaceShip(ship, occupied) {
    const maxAttempts = 100;
    for (let attempt = 0; attempt < maxAttempts; attempt++) {
        const horizontal = Math.random() < 0.5;
        const x = Math.floor(Math.random() * (horizontal ? GRID_SIZE - ship.size + 1 : GRID_SIZE));
        const y = Math.floor(Math.random() * (horizontal ? GRID_SIZE : GRID_SIZE - ship.size + 1));
        const cells = [];
        let valid = true;
        for (let i = 0; i < ship.size; i++) {
            const cx = x + (horizontal ? i : 0);
            const cy = y + (horizontal ? 0 : i);
            const index = coordToIndex(cx, cy);
            if (occupied.has(index) || hasAdjacentShip(cx, cy, occupied)) {
                valid = false;
                break;
            }
            cells.push(index);
        }
        if (valid) {
            cells.forEach((c) => occupied.add(c));
            console.log(`  Placed ${ship.name} (${ship.size}) at (${x},${y}) ${horizontal ? "H" : "V"}`);
            return true;
        }
    }
    return false;
}
function hasAdjacentShip(x, y, occupied) {
    const directions = [
        [-1, -1], [-1, 0], [-1, 1],
        [0, -1], [0, 1],
        [1, -1], [1, 0], [1, 1],
    ];
    for (const [dx, dy] of directions) {
        const nx = x + dx;
        const ny = y + dy;
        if (nx >= 0 && nx < GRID_SIZE && ny >= 0 && ny < GRID_SIZE) {
            if (occupied.has(coordToIndex(nx, ny))) {
                return true;
            }
        }
    }
    return false;
}
// AI Strategy: Hunt mode when hits exist, Random mode otherwise
export class AttackStrategy {
    hitCells = new Set();
    attackedCells = new Set();
    targetQueue = [];
    markAttacked(pos, isHit) {
        const index = coordToIndex(pos.x, pos.y);
        this.attackedCells.add(index);
        if (isHit) {
            this.hitCells.add(index);
            // Add adjacent cells to target queue
            this.addAdjacentTargets(pos);
        }
    }
    addAdjacentTargets(pos) {
        const directions = [
            [0, -1], [0, 1], [-1, 0], [1, 0], // Only cardinal directions
        ];
        for (const [dx, dy] of directions) {
            const x = pos.x + dx;
            const y = pos.y + dy;
            if (x >= 0 && x < GRID_SIZE && y >= 0 && y < GRID_SIZE) {
                const index = coordToIndex(x, y);
                if (!this.attackedCells.has(index)) {
                    const target = { x, y };
                    if (!this.targetQueue.some((t) => t.x === x && t.y === y)) {
                        this.targetQueue.push(target);
                    }
                }
            }
        }
    }
    getNextTarget() {
        // Hunt mode: target adjacent to hits
        if (this.targetQueue.length > 0) {
            const target = this.targetQueue.shift();
            const index = coordToIndex(target.x, target.y);
            if (!this.attackedCells.has(index)) {
                console.log(`🎯 Hunt mode: targeting (${target.x},${target.y})`);
                return target;
            }
            return this.getNextTarget(); // Try next in queue
        }
        // Search mode: random unattacked cell
        return this.getRandomTarget();
    }
    getRandomTarget() {
        const available = [];
        for (let y = 0; y < GRID_SIZE; y++) {
            for (let x = 0; x < GRID_SIZE; x++) {
                const index = coordToIndex(x, y);
                if (!this.attackedCells.has(index)) {
                    available.push({ x, y });
                }
            }
        }
        if (available.length === 0) {
            throw new Error("No available cells to attack");
        }
        const target = available[Math.floor(Math.random() * available.length)];
        console.log(`🔍 Search mode: random target (${target.x},${target.y})`);
        return target;
    }
}
// Helper function to place ships and generate grid
export function placeShipsRandomly() {
    const occupied = generateRandomShipPlacement();
    const cells = [];
    for (let i = 0; i < GRID_SIZE * GRID_SIZE; i++) {
        const salt = new Uint8Array(32);
        crypto.getRandomValues(salt);
        cells.push({
            salt,
            isOccupied: occupied.has(i),
        });
    }
    return cells;
}
// Helper function to select next attack target
export function selectAttackTarget(attacks) {
    const strategy = new AttackStrategy();
    // Populate strategy with previous attacks
    for (const [coord, isHit] of attacks.entries()) {
        const [x, y] = coord.split(',').map(Number);
        strategy.markAttacked({ x, y }, isHit);
    }
    return strategy.getNextTarget();
}
