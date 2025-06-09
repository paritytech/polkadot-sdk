import type { Presets } from ".";
import { getApis, logger, safeJsonStringify } from "./utils";

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

enum EventOutcome {
	Passed,
	Ignored,
	TimedOut,
	Done,
}

export class TestCase {
	eventSequence: Observe[];
	onComplete: () => void;

	constructor(e: Observe[], onComplete: () => void = () => {}) {
		this.eventSequence = e;
		this.onComplete = onComplete;
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

	onEvent(e: IEvent): EventOutcome {
		logger.debug(`Processing event: ${pe(e)}`);
		const expectedEvent = this.eventSequence[0]!;

		if (this.match(expectedEvent.e, e)) {
			expectedEvent.onPass();
			this.eventSequence.shift();
			logger.info(`Event passed`);
			if (this.eventSequence.length === 0) {
				logger.info("All events processed.");
				this.onComplete();
				return EventOutcome.Done;
			} else {
				logger.info(
					`Next expected event: ${this.eventSequence[0]!.toString()}, remaining: ${
						this.eventSequence.length
					}`
				);
				return EventOutcome.Passed;
			}
		} else if (this.notTimedOut(expectedEvent.e, e.block)) {
			logger.debug(`event not relevant, but not timed out`);
			return EventOutcome.Ignored;
		} else {
			logger.error(`Event not passed: expected ${expectedEvent} got ${pe(e)}, and timed out`);
			return EventOutcome.TimedOut;
		}
	}
}

export async function runTest(test: TestCase): Promise<void> {
	const { rcApi, paraApi, paraClient, rcClient } = getApis();

	logger.info(`Connecting to relay chain ${(await rcApi.constants.System.Version()).spec_name}`);
	logger.info(`Connecting to parachain ${(await paraApi.constants.System.Version()).spec_name}`);

	rcClient.finalizedBlock$.subscribe(async (block) => {
		const events = await rcApi.query.System.Events.getValue({ at: block.hash });
		events
			.filter(
				(e) =>
					e.event.type === "Session" ||
					e.event.type === "RootOffences" ||
					e.event.type === "StakingAhClient"
			)
			.map((e) => {
				return {
					chain: Chain.Relay,
					module: e.event.type,
					event: e.event.value.type,
					data: e.event.value.value,
					block: block.number,
				};
			})
			.forEach((e) => {
				test.onEvent(e);
			});
	});

	paraClient.finalizedBlock$.subscribe(async (block) => {
		const events = await paraApi.query.System.Events.getValue({ at: block.hash });
		events
			.filter(
				(e) =>
					e.event.type == "Staking" ||
					e.event.type == "MultiBlock" ||
					e.event.type == "MultiBlockSigned" ||
					e.event.type == "MultiBlockVerifier" ||
					e.event.type == "StakingRcClient"
			)
			.map((e) => {
				return {
					chain: Chain.Parachain,
					module: e.event.type,
					event: e.event.value.type,
					data: e.event.value.value,
					block: block.number,
				};
			})
			.forEach((e) => {
				test.onEvent(e);
			});
	});

	process.on("SIGINT", () => {
		console.log("Exiting on Ctrl+C...");
		rcClient.destroy();
		paraClient.destroy();
		process.exit(0);
	});
}
