import { test, expect } from "bun:test";
import { Presets } from "../src";
import { runPresetUntilLaunched } from "../src/cmd";
import { Chain, EventOutcome, Observe, runTest, TestCase } from "../src/test-case";
import { alice, getApis, GlobalTimeout, logger, nullifySigned, aliceStash, derivePubkeyFrom } from "../src/utils";

const PRESET: Presets = Presets.RealS;

test(
	`slashing spam test on ${PRESET}`,
	async () => {
		const { killZn, paraLog } = await runPresetUntilLaunched(PRESET);
		const apis = await getApis();
		let target = 10 * 1000;
		let batchSize = 1000; // Configurable batch size - adjust based on your needs
		let offenceCount = 0;
		let processedOffenceCount = 0;

		const steps = [
			// first relay session change at block 11, just a sanity check
			Observe.on(Chain.Relay, "Session", "NewSession")
				.byBlock(11)
				.onPass(async () => {
					logger.info(`Submitting ${target} offences in batches of ${batchSize}`);

					// Calculate number of batches needed
					const numBatches = Math.ceil(target / batchSize);

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
									BigInt(offenceIndex), // time slot
									100000000 // slash ppm
								]]
							}).decodedCall;
						});

						// Submit this batch as a single transaction
						try {
							const batchCall = apis.rcApi.tx.Utility.force_batch({ calls: offenceCalls }).decodedCall;
							const _result = apis.rcApi.tx.Sudo.sudo({ call: batchCall })
								.signAndSubmit(alice, { at: "finalized" });

							logger.info(`Batch ${batchIndex + 1} submitted`);
							offenceCount += currentBatchSize;

							// Small delay between batches to avoid overwhelming the system
							if (batchIndex < numBatches - 1) {
								logger.verbose(`Waiting 2s before next batch...`);
								await new Promise(resolve => setTimeout(resolve, 2000));
							}

						} catch (error) {
							logger.error(`Batch ${batchIndex + 1} failed:`, error);
							// Continue with next batch even if this one fails
						}
					}

					logger.info(`Completed submission of ${offenceCount} offences in ${numBatches} batches`);
				}),

			// Eventually see some slash computation (proving the system still works)
			Observe.on(Chain.Parachain, "WontReach", "SlashComputed")
		];

		const testCase = new TestCase(
			steps.map((s) => s.build()),
			true,
			() => {
				logger.info(`Test completed. Created ${offenceCount} offences, processed ${processedOffenceCount} in parachain`);

				// Log results for analysis
				logger.info("=== OFFENCE SPAM TEST RESULTS ===");
				logger.info(`Total offences created on relay chain: ${offenceCount}`);
				logger.info(`Total offences processed by parachain: ${processedOffenceCount}`);
				if (offenceCount > 0) {
					logger.info(`Spam reduction ratio: ${((offenceCount - processedOffenceCount) / offenceCount * 100).toFixed(1)}%`);
				}
				logger.info("Expected behavior: Multiple offences with unique time slots should trigger spam handling");
				logger.info("Check DMP queue status and ah-client deduplication effectiveness");

				killZn();
			}
		);

		const outcome = await runTest(testCase, apis, paraLog);
		expect(outcome).toEqual(EventOutcome.Done);

		// Verify that spam filtering occurred
		expect(processedOffenceCount).toBeLessThan(offenceCount);
		logger.info(`Successfully demonstrated spam filtering: ${offenceCount} -> ${processedOffenceCount} offences`);
	},
	{ timeout: GlobalTimeout * 2 } // Double timeout for this complex test
);
