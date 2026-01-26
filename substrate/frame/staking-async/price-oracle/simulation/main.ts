import { createLogger, format, transports } from "winston";
import { plotPriceComparison } from "./plot";
import { PRESETS, RealisticPriceGenerator } from "./realistic-price";

const logger = createLogger({
	level: process.env.LOG_LEVEL || "debug",
	format: format.combine(
		format.timestamp(),
		format.printf(({ level, message }) => {
			return `[${level}]: ${message}`;
		})
	),
	transports: [new transports.Console()],
});

// A validator vote on the next price
export class Vote {
	// who am I
	who: string;
	// the new price
	price: number

	constructor(who: string, price: number) {
		this.who = who;
		this.price = price;
	}
}

export class Chain {
	// The current onchian price.
	currentPrice: number;
	// previous onchain price state.
	priceHistory: Map<number, number>;
	// The current block number.
	currentBlock: number;
	// the unprocessed votes since `currentBlock`
	unprocessedVotes: Vote[];
	// All of the chain validators.
	validators: Validator[];
	// How we tally the votes.
	tally: (votes: Vote[], currentPrice: number) => number;

	constructor(tally: (votes: Vote[], currentPrice: number) => number, validators: Validator[]) {
		this.tally = tally;
		this.validators = validators;
		this.currentBlock = 0;
		this.currentPrice = 0;
		this.priceHistory = new Map();
		this.unprocessedVotes = [];
	}

	// Add a new vote to the `unprocessedVotes`
	addVote(vote: Vote): void {
		this.unprocessedVotes.push(vote);
	}

	// Throw the `currentPrice` into the `priceHistory`, and calculate a new `currentPrice` based on the `unprocessedVotes`
	processBlock(): void {
		for (const validator of this.validators) {
			this.addVote(validator.castVote());
		}
		const newPrice = this.tally(this.unprocessedVotes, this.currentPrice);
		this.priceHistory.set(this.currentBlock, this.currentPrice);
		this.currentPrice = newPrice;
		this.currentBlock++;
		this.unprocessedVotes = [];
	}
}

export class Validator {
	/// Who am I?
	who: string;
	/// Fetch a new price from an API now.
	fetchPrice: () => number;

	constructor(who: string, fetchPrice: () => number) {
		this.who = who;
		this.fetchPrice = fetchPrice;
	}

	/// Add a vote. This should be called at each block, if we assume the validator does the job flawlessly.
	/// Note that `fetchPrice` might do wonky things.
	castVote(): Vote {
		return new Vote(this.who, this.fetchPrice());
	}
}

/// A since function that oscilates between x and y.
export function sinWavePrice(time: number, x: number, y: number): number {
	const price = (Math.sin(time) + 1) / 2 * (y - x) + x;
	return price;
}

/// Take an `original` function and add a random error to it with a given `error_rate` and `error_max`.
export function withError(original: () => number, error_rate: number, error_max: number): number {
	if (Math.random() < error_rate) {
		return original() + Math.random() * error_max;
	} else {
		return original();
	}
}

class Tally {
	static faceValueAverage(votes: Vote[], _currentPrice: number): number {
		return votes.reduce((sum, vote) => sum + vote.price, 0) / votes.length;
	}

	static withMaxMove(votes: Vote[], currentPrice: number, maxMove: number): number {
		const newPrice = this.faceValueAverage(votes, currentPrice);
		const diff = newPrice - currentPrice;
		if (diff > maxMove) {
			return currentPrice + maxMove;
		} else if (diff < -maxMove) {
			return currentPrice - maxMove;
		} else {
			return newPrice;
		}
	}
}



async function main() {
	try {
		const blockTime = 6;
		let time = 0;
		const weekBlocks = 7 * 24 * 60 * 60 / blockTime;

		const gmb = new RealisticPriceGenerator(PRESETS.VOLATILE)

		const validators = [
			new Validator("validator1", () => withError(() => gmb.getPrice(time), 0, 0)),
			new Validator("validator2", () => withError(() => gmb.getPrice(time), 0, 0)),
			// 5% of the time, this validator is off by 1%
			new Validator("validator3", () => withError(() => gmb.getPrice(time), 0.05, 0.1)),
		];
		const chain = new Chain(Tally.faceValueAverage, validators);


		for (let block = 0; block < weekBlocks; block++) {
			chain.processBlock();
			time += blockTime;
			logger.info(`[#${block},t=${time}] onchain-price=${chain.currentPrice}, real-price=${gmb.getPrice(time)}`);
		}

		await plotPriceComparison(
			(time) => gmb.getPrice(time),
			chain,
			blockTime
		);
	} catch (error) {
		console.error("Error in main:", error);
		process.exit(1);
	}

	process.exit(0);
}

main();
