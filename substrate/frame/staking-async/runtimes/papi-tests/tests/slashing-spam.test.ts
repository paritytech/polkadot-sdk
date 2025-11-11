import { test, expect } from "bun:test";
import { Presets } from "../src";
import { runPresetUntilLaunched } from "../src/cmd";
import { Chain, EventOutcome, Observe, runTest, TestCase } from "../src/test-case";
import { alice, getApis, GlobalTimeout, logger, nullifySigned, aliceStash, derivePubkeyFrom, ss58 } from "../src/utils";

const PRESET: Presets = Presets.RealS;

test(
	`slashing spam test on ${PRESET}`,
	async () => {
		const { killZn, paraLog } = await runPresetUntilLaunched(PRESET);
		const apis = await getApis();
		// total number of offences to send.
		const target = 1000;
		// the size of each batch.
		const batchSize = 100;
		const numBatches = Math.ceil(target / batchSize);
		let sent = 0;
		let received = 0;
		// onchain-page-size for offence queueing in RC is 50, so we expect 20 pages for 1000 offences.

		const steps = [
			// first relay session change at block 11, just a sanity check
			Observe.on(Chain.Relay, "Session", "NewSession")
				.byBlock(11)
				.onPass(async () => {
					logger.info(`Submitting ${target} offences in batches of ${batchSize}`);

					// Calculate number of batches needed

					let nonce = await apis.rcApi.apis.AccountNonceApi.account_nonce(ss58(alice.publicKey));
					logger.info(`Alice nonce at start: ${nonce}`);

					for (let batchIndex = 0; batchIndex < numBatches; batchIndex++) {
						const start = batchIndex * batchSize;
						const end = Math.min(start + batchSize, target);
						const currentBatchSize = end - start;

						logger.info(`Processing batch ${batchIndex + 1}/${numBatches}: offences ${start} to ${end - 1}`);

						// Create batch of offence calls
						const offenceCalls = Array.from({ length: currentBatchSize }, (_, i) => {
							const offenceIndex = start + i;
							logger.debug(`Preparing offence ${offenceIndex}: ${derivePubkeyFrom(`//${offenceIndex}`)}`);

							return apis.rcApi.tx.RootOffences.report_offence({
								offences: [[
									[derivePubkeyFrom(`//${offenceIndex}`), { total: BigInt(0), own: BigInt(0), others: [] }],
									0, // session index
									BigInt(offenceIndex), // time slot, each being unique
									100000000 // slash ppm
								]]
							}).decodedCall;
						});

						// Submit this batch as a single transaction
						try {
							const batchCall = apis.rcApi.tx.Utility.force_batch({ calls: offenceCalls }).decodedCall;
							const result = apis.rcApi.tx.Sudo.sudo({ call: batchCall })
								.signAndSubmit(alice, { at: "best", nonce: nonce });

							logger.info(`Batch ${batchIndex + 1} submitted`);
							nonce += 1;
							sent += currentBatchSize;

						} catch (error) {
							logger.error(`Batch ${batchIndex + 1} failed:`, error);
							// Continue with next batch even if this one fails
						}
					}
				}),

			// in the meantime, we expect to see on the AH side:
			...Array.from({ length: 20 }, (_, __) =>
				Observe.on(Chain.Parachain, "StakingRcClient", "OffenceReceived").withDataCheck((x) => {
					received += x.offences_count;
					return true
				}),
			),
		];

		const testCase = new TestCase(
			steps.map((s) => s.build()),
			true,
			() => {
				logger.info(`Test completed. Created ${sent} offences, processed ${received} in parachain`);
				killZn();
			}
		);

		const outcome = await runTest(testCase, apis, paraLog);
		expect(outcome).toEqual(EventOutcome.Done);
		expect(sent).toEqual(received);
	},
	{ timeout: GlobalTimeout * 2 } // Double timeout for this complex test
);
