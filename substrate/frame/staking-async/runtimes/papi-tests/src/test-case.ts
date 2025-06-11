import { logger, safeJsonStringify, type ApiDeclerations } from "./utils";
import { exit } from "process";

export enum Chain {
	Relay = "Relay",
	Parachain = "Parachain",
}

interface IEvent {
	module: string;
	event: string;
	data: any | undefined;
}

// Print an event.
function pe(e: IEvent): string {
	return `${e.module} ${e.event} ${e.data ? safeJsonStringify(e.data) : "no data"}`;
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

	// Static builder entry point
	static on(chain: Chain, mod: string, event: string): ObserveBuilder {
		return new ObserveBuilder(chain, mod, event);
	}
}

export class ObserveBuilder {
	private chain: Chain;
	private module: string;
	private event: string;
	private dataCheck?: (data: any) => boolean;
	private byBlockVal?: number;
	private onPassCallback: () => void = () => {};

	constructor(chain: Chain, module: string, event: string) {
		this.chain = chain;
		this.module = module;
		this.event = event;
	}

	withDataCheck(check: (data: any) => boolean): ObserveBuilder {
		this.dataCheck = check;
		return this;
	}

	byBlock(blockNumber: number): ObserveBuilder {
		this.byBlockVal = blockNumber;
		return this;
	}

	onPass(callback: () => void): ObserveBuilder {
		this.onPassCallback = callback;
		return this;
	}

	build(): Observe {
		if (!this.module || !this.event) {
			throw new Error("Module and event are required");
		}

		return new Observe(
			this.chain,
			this.module,
			this.event,
			this.dataCheck,
			this.byBlockVal,
			this.onPassCallback
		);
	}
}

export enum EventOutcome {
	TimedOut = "TimedOut",
	Done = "Done",
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

	match(ours: IObservableEvent, theirs: IEvent, theirsChain: Chain): boolean {
		const trivialComp =
			ours.chain === theirsChain &&
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

	onBlock(chain: Chain, block: number, weights: any, events: IEvent[]) {
		// sort from small to big
		logger.debug(`Processing ${chain} block ${block}, events: ${events.length}`);
		const firstTimeOut = this.eventSequence
			.filter((e) => e.e.byBlock)
			.map((e) => e.e.byBlock!)
			.sort((x, y) => x - y);
		if (firstTimeOut.length > 0 && block > firstTimeOut[0]!) {
			logger.error(
				`Block ${block} is past the first timeout at block ${firstTimeOut[0]}, exiting.`
			);
			this.resolveTestPromise(EventOutcome.TimedOut);
		}

		for (const e of events) {
			this.onEvent(e, chain);
		}
	}

	onEvent(e: IEvent, chain: Chain) {
		if (!this.eventSequence.length) {
			logger.warn(`No events to process for ${chain}, event: ${pe(e)}`);
			return;
		}
		logger.verbose(`Processing ${chain} event: ${pe(e)}`);
		const [primary, maybeSecondary] = this.nextEvent(chain);

		if (this.match(primary.e, e, chain)) {
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
		} else if (maybeSecondary && this.match(maybeSecondary.e, e, chain)) {
			maybeSecondary.onPass();
			this.removeEvent(maybeSecondary);
			logger.info(`Secondary event passed`);
			// when we check secondary events, we must have at least 2 items in the list, so no
			// need to check for the end of list.
		} else {
			logger.verbose(`event not relevant`);
		}
	}
}

export async function runTest(test: TestCase, apis: ApiDeclerations): Promise<EventOutcome> {
	const { rcClient, paraClient, rcApi, paraApi } = apis;

	let completionPromise: Promise<EventOutcome> = new Promise((resolve, _) => {
		// Pass the resolve/reject functions to the TestCase instance
		test.setTestPromiseResolvers(resolve);

		rcClient.finalizedBlock$.subscribe(async (block) => {
			const events = await rcApi.query.System.Events.getValue({ at: block.hash });
			const weights = await rcApi.query.System.BlockWeight.getValue({ at: block.hash });
			const interested = events
				.filter(
					(e) =>
						e.event.type === "Session" ||
						e.event.type === "RootOffences" ||
						e.event.type === "StakingAhClient"
				)
				.map((e) => ({
					module: e.event.type,
					event: e.event.value.type,
					data: e.event.value.value,
				}));
			test.onBlock(Chain.Relay, block.number, weights, interested);
		});

		paraClient.blocks$.subscribe(async (block) => {
			const events = await paraApi.query.System.Events.getValue({ at: block.hash });
			const weights = await paraApi.query.System.BlockWeight.getValue({ at: block.hash });
			const interested = events
				.filter(
					(e) =>
						e.event.type == "Staking" ||
						e.event.type == "MultiBlockElection" ||
						e.event.type == "MultiBlockElectionSigned" ||
						e.event.type == "MultiBlockElectionVerifier" ||
						e.event.type == "StakingRcClient"
				)
				.map((e) => ({
					module: e.event.type,
					event: e.event.value.type,
					data: e.event.value.value,
				}));
			test.onBlock(Chain.Parachain, block.number, weights, interested);
		});
	});

	// Handle graceful exit on SIGINT
	process.on("SIGINT", () => {
		console.log("Exiting on Ctrl+C...");
		rcClient.destroy();
		paraClient.destroy();
		test.onKill();
		process.exit(0);
	});

	// Wait for the completionPromise to resolve/reject
	const finalOutcome = await completionPromise;
	rcClient.destroy();
	paraClient.destroy();
	test.onKill();
	return finalOutcome;
}
