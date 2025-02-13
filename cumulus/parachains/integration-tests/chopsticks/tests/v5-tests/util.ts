import { expect } from "bun:test";
import type { wnd_rc } from "@polkadot-api/descriptors";
import {
	type ChainDefinition,
	type PolkadotClient,
	type TypedApi,
	createClient,
} from "polkadot-api";
import { withPolkadotSdkCompat } from "polkadot-api/polkadot-sdk-compat";
import { getWsProvider } from "polkadot-api/ws-provider/web";

import { take } from "rxjs";

export interface ClientContext<D extends ChainDefinition> {
	client: PolkadotClient;
	api: TypedApi<D>;
}

export function createPolkadotClient<D extends ChainDefinition>(
	wsAddress: string,
	apiType: D,
): ClientContext<D> {
	const client = createClient(withPolkadotSdkCompat(getWsProvider(wsAddress)));
	return { client: client, api: client.getTypedApi(apiType) };
}

type InferRequiredApi<T, P extends string[]> = P extends [
	infer Head extends keyof T,
	...infer Tail extends string[],
]
	? { [K in Head]: InferRequiredApi<T[K], Tail> }
	: T;

type RequiredApi = InferRequiredApi<
	TypedApi<typeof wnd_rc>,
	["query", "System", "Account", "getValue"]
>;

export async function getFreeBalance(api: RequiredApi, accountKey: string) {
	const balance = await api.query.System.Account.getValue(accountKey);
	return balance.data.free;
}

export async function waitForFinalizedBlocks<T>(
	client: PolkadotClient,
	count: number,
	action: () => T | Promise<T>,
): Promise<T> {
	return new Promise<T>((resolve) => {
		let receivedCount = 0;
		let isFirst = true;
		let actionResult: T;

		client.finalizedBlock$.pipe(take(count + 1)).subscribe({
			next: async () => {
				if (isFirst) {
					isFirst = false;
					actionResult = await action();
				} else {
					receivedCount++;
					if (receivedCount === count) {
						resolve(actionResult);
					}
				}
			},
		});
	});
}

export function assert(condition: unknown, msg?: string): asserts condition {
	expect(condition).toBe(true);
	// following is just to satisfy TS type narrowing since testing framework's expect() doesn't provide this
	if (condition === false) throw new Error(msg);
}
