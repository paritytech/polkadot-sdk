import { test, expect } from "bun:test";
import { Presets } from "../src";
import { runPresetUntilLaunched } from "../src/cmd";
import { Chain, EventOutcome, Observe, runTest, TestCase } from "../src/test-case";
import { alice, aliceStash, deriveFrom, getApis, GlobalTimeout, logger, safeJsonStringify, ss58, type ApiDeclarations } from "../src/utils";
import { DEV_PHRASE } from "@polkadot-labs/hdkd-helpers";
import { FixedSizeBinary, type PolkadotSigner, type TxCallData, type TypedApi } from "polkadot-api";
import { rc } from "@polkadot-api/descriptors";

const PRESET: Presets = Presets.FakeDev;

// 0x6e002802080a08001c90e3dabd3fd0f6bc648045018f78fcee8fe24122c22d8d2a14e9905073d10f0f0080c6a47e8d03280300011c90e3dabd3fd0f6bc648045018f78fcee8fe24122c22d8d2a14e9905073d10f1f0105010005000101001c90e3dabd3fd0f6bc648045018f78fcee8fe24122c22d8d2a14e9905073d10f05040000000b00a0724e180900000000

async function sendDown(api: TypedApi<typeof rc>, count: number) {
	const calls: TxCallData[] = [];
	for (let i = 0; i < count; i++) {
		const account = deriveFrom(DEV_PHRASE, `//${i}`)
		const endowment = BigInt(10e12);
		const teleport = BigInt(10e11);

		const forceSetBalance = api.tx.Balances.force_set_balance({
			new_free: endowment,
			who: { type: "Id", value: ss58(account.publicKey) }
		})

		const xcm = api.tx.XcmPallet.teleport_assets({
			dest: {
				type: "V5",
				value: {
					parents: 0,
					interior: {
						type: "X1",
						value: { type: "Parachain", value: 1100 }
					}
				}
			},
			beneficiary: {
				type: "V5",
				value: {
					parents: 0,
					interior: {
						type: "X1",
						value: {
							type: "AccountId32",
							value: { id: new FixedSizeBinary(account.publicKey) }
						}
					}
				}
			},
			assets: {
				type: "V5",
				value: [
					{
						id: {
							parents: 0,
							interior: {
								type: "Here",
								value: undefined
							}
						},
						fun: {
							type: "Fungible",
							value: teleport
						}
					}
				]
			},
			fee_asset_item: 0,
		})
		const dispatchAs = api.tx.Utility.dispatch_as({
			as_origin: { type: "system", value: { type: "Signed", value: ss58(account.publicKey) } },
			call: xcm.decodedCall
		});
		calls.push(forceSetBalance.decodedCall);
		calls.push(dispatchAs.decodedCall);
	}

	const finalBatch = api.tx.Utility.batch_all({ calls });
	const finalSudo = api.tx.Sudo.sudo({ call: finalBatch.decodedCall });
	const res = await finalSudo.signAndSubmit(alice, { at: "best" });
	let success = 0;
	let failure = 0;
	res.events.forEach((e) => {
		logger.debug(safeJsonStringify(e.value));
		if (e.value.type === "DispatchedAs") {
			// @ts-ignore
			if (e.value.value.result.success) {
				success += 1
			} else {
				failure += 1
			}
		}
	});
	logger.info(`Sent ${count} downward messages, intercepted ${success + failure} events, ${success} succeeded, ${failure} failed`);
}

test(
	`${PRESET} preset with vmp queues being spammed af`,
	async () => {
		const { killZn, paraLog } = await runPresetUntilLaunched(PRESET);

		const apis = await getApis();

		const sub = apis.rcClient.blocks$.subscribe((block) => {
			if (block.number > 10) {
				logger.verbose(`spammer:: spamming at height ${block.number}`);
				sendDown(apis.rcApi, 40);
			}
		});
		const steps: Observe[] = [
			Observe.on(Chain.Relay, "Session", "NewSession")
				.byBlock(11),
				// .onPass(async () => {
				// 	await sendDown(apis.rcApi, 40);
				// }),
			Observe.on(Chain.Relay, "WontReach", "WontReach")
		].map((s) => s.build());

		const testCase = new TestCase(steps, true, () => {
			killZn();
			sub.unsubscribe();
		});

		const outcome = await runTest(testCase, apis, paraLog);
		expect(outcome).toEqual(EventOutcome.Done);
	},
	{ timeout: GlobalTimeout }
);
