import { rcPresetFor, runPreset } from "./cmd";
import { logger } from "./utils";
import { monitorDmpQueue } from "./dmp-monitor";
import { Command } from "commander";

export enum Presets {
	FakeDev = "fake-dev",
	FakeDot = "fake-dot",
	FakeKsm = "fake-ksm",
	RealS = "real-s",
	RealM = "real-m",
}

if (require.main === module) {
	const program = new Command();
	program
		.name("staking-async-papi-tests")
		.description("Run staking-async PAPI tests")
		.version("0.1.0");

	program
		.command("run")
		.description("Run a given preset. This just sets up the ZN env and runs it")
		.option(
			"-p, --para-preset <preset>",
			"run the given parachain preset. The right relay preset, and zn-toml file are auto-chosen.",
			Presets.FakeDev
		)
		.action(async (options) => {
			const { paraPreset } = options;
			runPreset(paraPreset);
		});

	program
		.command("monitor-dmp")
		.description("Monitor DMP (Downward Message Passing) queue status and metrics")
		.option(
			"-p, --port <port>",
			"WebSocket port to connect to the chain",
			"9944"
		)
		.option(
			"-r, --refresh <seconds>",
			"Refresh interval in seconds",
			"5"
		)
		.option(
			"--para-id <id>",
			"Specific parachain ID to monitor (default: 1100)"
		)
		.action(async (options) => {
			const { port, refresh, paraId } = options;
			await monitorDmpQueue({
				port: parseInt(port),
				refreshInterval: parseInt(refresh),
				paraId: paraId ? parseInt(paraId) : 1100
			});
		});

	program.parse(process.argv);
}
