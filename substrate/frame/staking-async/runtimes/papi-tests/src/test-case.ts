import { logger, safeJsonStringify, type ApiDeclerations } from "./utils";
import { exit } from "process";

export enum Chain {
	Relay = "Relay",
	Parachain = "Parachain",
}

interface IEvent {
	chain: Chain;
	module: string;
	event: string;
	data: any | undefined;
	block: number;
}

// Print an event.
function pe(e: IEvent): string {
	return `${e.chain} ${e.module} ${e.event} ${
		e.data ? safeJsonStringify(e.data) : "no data"
	} at block ${e.block}`;
}

interface IObservableEvent {
	chain: Chain;
	module: string;
	event: string;
	dataCheck: ((data: any) => boolean) | undefined;
	byBlock: number | undefined;
}

export class Observe {
	e: IObservableEvent;
	onPass: () => void = () => {};

	constructor(
		chain: Chain,
		module: string,
		event: string,
		dataCheck: ((data: any) => boolean) | undefined = undefined,
		byBlock: number | undefined = undefined,
		onPass: () => void = () => {}
	) {
		this.e = { chain, module, event, dataCheck, byBlock };
		this.onPass = onPass;
	}

	toString(): string {
		return `Observe(${this.e.chain}, ${this.e.module}, ${this.e.event}, ${
			this.e.dataCheck ? "dataCheck" : "no dataCheck"
		}, ${this.e.byBlock ? this.e.byBlock : "no byBlock"})`;
	}
}

export enum EventOutcome {
	Passed,
	Ignored,
	TimedOut,
	Done,
}

export class TestCase {
	eventSequence: Observe[];
	onKill: () => void;
	allowPerChainInterleavedEvents: boolean = false;
	private resolveTestPromise: (outcome: EventOutcome) => void = () => {};

	constructor(e: Observe[], interleave: boolean = false, onKill: () => void = () => {}) {
		this.eventSequence = e;
		this.onKill = onKill;
		this.allowPerChainInterleavedEvents = interleave;
	}

	// New: Method to set the promise resolvers
	setTestPromiseResolvers(resolve: (outcome: EventOutcome) => void) {
		this.resolveTestPromise = resolve;
	}

	match(ours: IObservableEvent, theirs: IEvent): boolean {
		const trivialComp =
			ours.chain === theirs.chain &&
			ours.module === theirs.module &&
			ours.event === theirs.event;
		if (trivialComp) {
			// note: only run data check if it is defined and all other criteria match
			const dataComp = ours.dataCheck === undefined ? true : ours.dataCheck!(theirs.data);
			return trivialComp && dataComp;
		} else {
			return false;
		}
	}

	notTimedOut(ours: IObservableEvent, block: number): boolean {
		return ours.byBlock === undefined ? true : block <= ours.byBlock;
	}

	// returns a [`primary`, `maybeSecondary`] event to check. `primary` should always be checked first, and if not secondary is checked.
	nextEvent(chain: Chain): [Observe, Observe | undefined] {
		const next = this.eventSequence[0]!;
		if (this.allowPerChainInterleavedEvents && this.eventSequence.length > 1) {
			// get the next event in our list that is of type `chain`.
			const nextOfChain = this.eventSequence.slice(1).find((e) => e.e.chain === chain);
			return [next, nextOfChain];
		} else {
			return [next, undefined];
		}
	}

	removeEvent(e: Observe): void {
		const index = this.eventSequence.findIndex((x) => x.e === e.e);
		if (index !== -1) {
			this.eventSequence.splice(index, 1);
		} else {
			logger.warn(`Event not found for removal: ${e.toString()}`);
			exit(1);
		}
	}

	onEvent(e: IEvent) {
		logger.debug(`Processing event: ${pe(e)}`);
		const [primary, maybeSecondary] = this.nextEvent(e.chain);

		if (this.match(primary.e, e)) {
			primary.onPass();
			this.removeEvent(primary);
			logger.info(`Primary event passed`);
			if (this.eventSequence.length === 0) {
				logger.info("All events processed.");
				this.resolveTestPromise(EventOutcome.Done);
			} else {
				const nextExpected = logger.info(
					`Next expected event: ${this.eventSequence[0]!.toString()}, remaining: ${
						this.eventSequence.length
					}`
				);
			}
		} else if (maybeSecondary && this.match(maybeSecondary.e, e)) {
			maybeSecondary.onPass();
			this.removeEvent(maybeSecondary);
			logger.info(`Secondary event passed`);
		} else if (this.notTimedOut(primary.e, e.block)) {
			logger.debug(`event not relevant, but not timed out`);
		} else {
			logger.error(
				`Event not passed: expected ${primary.e} or ${maybeSecondary?.e}, got ${pe(
					e
				)}, and timed out`
			);
			this.resolveTestPromise(EventOutcome.TimedOut);
		}
	}
}

export async function runTest(test: TestCase, apis: ApiDeclerations): Promise<EventOutcome> {
	const { rcClient, paraClient, rcApi, paraApi } = apis;

	let completionPromise: Promise<EventOutcome> = new Promise((resolve, _) => {
		// Pass the resolve/reject functions to the TestCase instance
		test.setTestPromiseResolvers(resolve);

		const subscribeToRelay = async () => {
			rcClient.finalizedBlock$.subscribe(async (block) => {
				const events = await rcApi.query.System.Events.getValue({ at: block.hash });
				for (const e of events) {
					// Use for...of for async iteration if needed, or simple forEach
					if (
						e.event.type === "Session" ||
						e.event.type === "RootOffences" ||
						e.event.type === "StakingAhClient"
					) {
						test.onEvent({
							chain: Chain.Relay,
							module: e.event.type,
							event: e.event.value.type,
							data: e.event.value.value,
							block: block.number,
						});
					}
				}
			});
		};

		const subscribeToParachain = async () => {
			paraClient.finalizedBlock$.subscribe(async (block) => {
				const events = await paraApi.query.System.Events.getValue({ at: block.hash });
				for (const e of events) {
					// Use for...of for async iteration if needed
					if (
						e.event.type == "Staking" ||
						e.event.type == "MultiBlock" ||
						e.event.type == "MultiBlockSigned" ||
						e.event.type == "MultiBlockVerifier" ||
						e.event.type == "StakingRcClient"
					) {
						test.onEvent({
							chain: Chain.Parachain,
							module: e.event.type,
							event: e.event.value.type,
							data: e.event.value.value,
							block: block.number,
						});
					}
				}
			});
		};

		subscribeToRelay();
		subscribeToParachain();
	});

	// Handle graceful exit on SIGINT
	process.on("SIGINT", () => {
		console.log("Exiting on Ctrl+C...");
		test.onKill();
		rcClient.destroy();
		paraClient.destroy();
		process.exit(0);
	});

	// Wait for the completionPromise to resolve/reject
	const finalOutcome = await completionPromise;
	return finalOutcome;
}
