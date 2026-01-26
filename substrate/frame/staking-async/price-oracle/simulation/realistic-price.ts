/**
 * Realistic price feed generator using Geometric Brownian Motion (GBM)
 * with stochastic volatility for simulating crypto token pair prices.
 *
 * Features:
 * - Periods of low and high volatility (volatility clustering)
 * - Drift component for trending behavior
 * - Prevents negative prices using log-returns
 * - Deterministic based on time for reproducibility
 */

/**
 * Seeded pseudo-random number generator (Mulberry32)
 * Returns a function that generates deterministic random numbers [0, 1)
 */
function seededRandom(seed: number): () => number {
	let state = seed;
	return function() {
		state = (state + 0x6D2B79F5) | 0;
		let t = Math.imul(state ^ (state >>> 15), 1 | state);
		t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
		return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
	};
}

/**
 * Box-Muller transform to generate standard normal distribution
 * from uniform random numbers
 */
function normalRandom(rng: () => number): number {
	const u1 = rng();
	const u2 = rng();
	return Math.sqrt(-2 * Math.log(u1)) * Math.cos(2 * Math.PI * u2);
}

/**
 * Configuration for the realistic price generator
 */
export interface PriceConfig {
	/** Initial price at time=0 */
	initialPrice: number;
	/** Weekly drift (mu) - typical range: -0.1 to 0.1 for crypto */
	drift: number;
	/** Base volatility (sigma) per week - typical range: 0.05 to 0.3 for crypto */
	baseVolatility: number;
	/** Volatility of volatility - controls how much volatility changes */
	volOfVol: number;
	/** Mean reversion speed for volatility - how fast volatility returns to base */
	volMeanReversion: number;
	/** Time step between observations (in seconds) */
	timeStep: number;
	/** Seed for reproducibility */
	seed?: number;
}

/**
 * Default configuration for a moderately volatile crypto pair
 */
export const DEFAULT_CONFIG: PriceConfig = {
	initialPrice: 5,
	drift: 0.0, // No drift = random walk
	baseVolatility: 0.30, // 30% weekly volatility
	volOfVol: 0.3, // Volatility can vary ±30%
	volMeanReversion: 0.1, // Slow mean reversion
	timeStep: 6, // 6 seconds (like block time)
	seed: 42,
};

/**
 * Price generator class that maintains state across time steps
 */
export class RealisticPriceGenerator {
	private config: PriceConfig;
	private rng: () => number;
	private cache: Map<number, { price: number; volatility: number }>;

	constructor(config: Partial<PriceConfig> = {}) {
		this.config = { ...DEFAULT_CONFIG, ...config };
		this.rng = seededRandom(this.config.seed || 42);
		this.cache = new Map();

		// Initialize at time 0
		this.cache.set(0, {
			price: this.config.initialPrice,
			volatility: this.config.baseVolatility,
		});
	}

	/**
	 * Get price at a specific time
	 * Uses GBM with stochastic volatility
	 */
	getPrice(time: number): number {
		// For time 0, return initial price
		if (time === 0) {
			return this.config.initialPrice;
		}

		// Check cache first
		if (this.cache.has(time)) {
			return this.cache.get(time)!.price;
		}

		// Find the last cached time before this time
		const cachedTimes = Array.from(this.cache.keys()).sort((a, b) => a - b);
		let lastTime = 0;
		for (const t of cachedTimes) {
			if (t < time) {
				lastTime = t;
			} else {
				break;
			}
		}

		// Generate prices from lastTime to time
		let currentTime = lastTime;
		let { price: currentPrice, volatility: currentVolatility } = this.cache.get(lastTime)!;

		while (currentTime < time) {
			const nextTime = currentTime + this.config.timeStep;
			const dt = this.config.timeStep / (7 * 24 * 60 * 60); // Convert to weeks

			// Generate random shocks
			const priceShock = normalRandom(this.rng);
			const volShock = normalRandom(this.rng);

			// Update volatility using mean-reverting process (Ornstein-Uhlenbeck)
			const volDrift = this.config.volMeanReversion * (this.config.baseVolatility - currentVolatility) * dt;
			const volDiffusion = this.config.volOfVol * Math.sqrt(dt) * volShock;
			currentVolatility = Math.max(0.01, currentVolatility + volDrift + volDiffusion);

			// Update price using GBM with current volatility
			const drift = this.config.drift * dt;
			const diffusion = currentVolatility * Math.sqrt(dt) * priceShock;
			const logReturn = drift - 0.5 * currentVolatility * currentVolatility * dt + diffusion;
			currentPrice = currentPrice * Math.exp(logReturn);

			// Cache the result
			this.cache.set(nextTime, { price: currentPrice, volatility: currentVolatility });
			currentTime = nextTime;
		}

		return this.cache.get(time)?.price || currentPrice;
	}

	/**
	 * Get the current volatility at a specific time
	 */
	getVolatility(time: number): number {
		this.getPrice(time); // Ensure price is calculated
		return this.cache.get(time)?.volatility || this.config.baseVolatility;
	}

	/**
	 * Clear the cache (useful for resetting)
	 */
	reset(): void {
		this.cache.clear();
		this.cache.set(0, {
			price: this.config.initialPrice,
			volatility: this.config.baseVolatility,
		});
		this.rng = seededRandom(this.config.seed || 42);
	}
}

/**
 * Simple function interface (similar to sinWavePrice)
 * Creates a new generator for each call - use this for quick testing
 * For production, create a RealisticPriceGenerator instance and reuse it
 *
 * @param time Time in seconds
 * @param initialPrice Starting price
 * @param drift Weekly drift (e.g., 0.05 = 5% per week)
 * @param baseVolatility Weekly volatility (e.g., 0.1 = 10% per week)
 */
export function realisticPrice(
	time: number,
	initialPrice: number = 5,
	drift: number = 0.0,
	baseVolatility: number = 0.1
): number {
	const generator = new RealisticPriceGenerator({
		initialPrice,
		drift,
		baseVolatility,
		timeStep: 6,
	});
	return generator.getPrice(time);
}

/**
 * Example preset configurations for different market conditions
 * All values are on a weekly basis
 */
export const PRESETS = {
	/** Calm market - low volatility */
	CALM: {
		...DEFAULT_CONFIG,
		baseVolatility: 0.05, // 5% weekly volatility
		volOfVol: 0.1,
	} as PriceConfig,

	/** Normal crypto market */
	NORMAL: {
		...DEFAULT_CONFIG,
		baseVolatility: 0.1, // 10% weekly volatility
		volOfVol: 0.3,
	} as PriceConfig,

	/** Volatile market - high volatility */
	VOLATILE: {
		...DEFAULT_CONFIG,
		baseVolatility: 0.2, // 20% weekly volatility
		volOfVol: 0.5,
	} as PriceConfig,

	/** Bull market - upward drift */
	BULL: {
		...DEFAULT_CONFIG,
		drift: 0.05, // 5% weekly drift upward
		baseVolatility: 0.12,
	} as PriceConfig,

	/** Bear market - downward drift */
	BEAR: {
		...DEFAULT_CONFIG,
		drift: -0.05, // 5% weekly drift downward
		baseVolatility: 0.15,
	} as PriceConfig,
};
