import { rcPresetFor, runPreset } from "./cmd";
import { logger } from "./utils";
import { monitorVmpQueues } from "./vmp-monitor";
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
		.command("monitor-vmp")
		.description("Monitor VMP (Vertical Message Passing) - both DMP and UMP queues")
		.option(
			"--relay-port <port>",
			"Relay chain WebSocket port",
			"9944"
		)
		.option(
			"--para-port <port>",
			"Parachain WebSocket port (optional)",
			"9946"
		)
		.option(
			"-r, --refresh <seconds>",
			"Refresh interval in seconds",
			"3"
		)
		.option(
			"--para-id <id>",
			"Specific parachain ID to monitor (default: all)"
		)
		.action(async (options) => {
			const { relayPort, paraPort, refresh, paraId } = options;
			await monitorVmpQueues({
				relayPort: parseInt(relayPort),
				paraPort: paraPort ? parseInt(paraPort) : undefined,
				refreshInterval: parseInt(refresh),
				paraId: paraId ? parseInt(paraId) : undefined
			});
		});

	program.parse(process.argv);
}
