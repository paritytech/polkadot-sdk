import { Chain, Observe } from "../src/test-case";
import { logger, nullifySigned, nullifyUnsigned, type ApiDeclarations } from "../src/utils";

// An unsigned solution scenario:
//
// When no staking-miner is running (and for simplicity the signed phase is also set to zero). We
// expect an unsigned solution to successfullly proceed and submit a solution with `minerPages` out
// of the total `pages`.
export function commonUnsignedSteps(
	expectedValidatorSetCount: number,
	minerPages: number,
	pages: number,
	doNullifySigned: boolean,
	apis: ApiDeclarations
): Observe[] {
	return [
		// first relay session change at block 11
		Observe.on(Chain.Relay, "Session", "NewSession").byBlock(11),
		// by block 10 we will plan a new era
		Observe.on(Chain.Parachain, "Staking", "SessionRotated")
			.withDataCheck((x: any) => x.active_era == 0 && x.planned_era == 1)
			.onPass(() => {
				if (doNullifySigned) {
					nullifySigned(apis.paraApi).then((ok) => {
						logger.verbose("Nullified signed phase:", ok);
					});
				}
			}),
		// eventually we will verify all pages
		...Array.from({ length: minerPages }, (_, __) => {
			return Observe.on(Chain.Parachain, "MultiBlockElectionVerifier", "Verified");
		}),
		// eventually it will be queued
		Observe.on(Chain.Parachain, "MultiBlockElectionVerifier", "Queued"),
		// eventually multiblock election will transition to `Done`
		Observe.on(Chain.Parachain, "MultiBlockElection", "PhaseTransitioned").withDataCheck(
			(x: any) => x.to.type === "Done"
		),
		// eventually we will export all 4 pages to staking
		// export events.
		...Array.from({ length: pages }, (_, __) => {
			return Observe.on(Chain.Parachain, "Staking", "PagedElectionProceeded");
		}),
		// eventually multiblock goes back to `Off`
		Observe.on(Chain.Parachain, "MultiBlockElection", "PhaseTransitioned").withDataCheck(
			(x: any) => x.to.type === "Off"
		),
		// eventually we will send it back to RC
		Observe.on(Chain.Relay, "StakingAhClient", "ValidatorSetReceived").withDataCheck(
			(x: any) => x.id === 1 && x.new_validator_set_count === expectedValidatorSetCount
		),
		Observe.on(Chain.Relay, "Session", "NewQueued"),
		// eventually we will receive a session report back in AH with activation timestamp
		Observe.on(Chain.Parachain, "StakingRcClient", "SessionReportReceived").withDataCheck(
			(x) => x.activation_timestamp !== undefined
		),
		// eventually we will have era paid (inflation)
		Observe.on(Chain.Parachain, "Staking", "EraPaid"),
	].map((s) => s.build());
}

// A signed solution scenario.
//
// This test expect you to call `spawnMiner` in the final test code. A full solution of `pages` is
// expected to be submitted.
export function commonSignedSteps(
	pages: number,
	expectedValidatorSetCount: number,
	apis: ApiDeclarations
): Observe[] {
	return [
		// first relay session change at block 11
		Observe.on(Chain.Relay, "Session", "NewSession").byBlock(11),
		// by block 10 we will plan a new era
		Observe.on(Chain.Parachain, "Staking", "SessionRotated")
			.withDataCheck((x: any) => x.active_era == 0 && x.planned_era == 1)
			.onPass(() => {
				nullifyUnsigned(apis.paraApi).then((ok) => {
					logger.verbose("Nullified unsigned phase:", ok);
				});
			}),

		// Eventually a signed submission is registered...
		Observe.on(Chain.Parachain, "MultiBlockElectionSigned", "Registered"),
		// ... and exact number of pages are generated
		...Array.from({ length: pages }, () =>
			Observe.on(Chain.Parachain, "MultiBlockElectionSigned", "Stored")
		),
		// ... and exact number of pages are verified
		...Array.from({ length: pages }, () =>
			Observe.on(Chain.Parachain, "MultiBlockElectionVerifier", "Verified")
		),
		// eventually it will be queued
		Observe.on(Chain.Parachain, "MultiBlockElectionVerifier", "Queued"),
		// eventually the signed submitter is rewarded.
		// TODO: check rewarded account is Bob
		Observe.on(Chain.Parachain, "MultiBlockElectionSigned", "Rewarded"),
		// eventually multiblock election will transition to `Done`
		Observe.on(Chain.Parachain, "MultiBlockElection", "PhaseTransitioned").withDataCheck(
			(x: any) => x.to.type === "Done"
		),
		// eventually we will export all pages.
		...Array.from({ length: pages }, () =>
			Observe.on(Chain.Parachain, "Staking", "PagedElectionProceeded")
		),
		// eventually multiblock goes back to `Off`
		Observe.on(Chain.Parachain, "MultiBlockElection", "PhaseTransitioned").withDataCheck(
			(x: any) => x.to.type === "Off"
		),
		// eventually we will send it back to RC
		Observe.on(Chain.Relay, "StakingAhClient", "ValidatorSetReceived").withDataCheck(
			(x: any) => x.id === 1 && x.new_validator_set_count === expectedValidatorSetCount
		),
		Observe.on(Chain.Relay, "Session", "NewQueued"),
		// eventually we will receive a session report back in AH with activation timestamp
		Observe.on(Chain.Parachain, "StakingRcClient", "SessionReportReceived").withDataCheck(
			(x) => x.activation_timestamp !== undefined
		),
		// eventually we will have era paid (inflation)
		Observe.on(Chain.Parachain, "Staking", "EraPaid"),
	].map((s) => s.build());
}
