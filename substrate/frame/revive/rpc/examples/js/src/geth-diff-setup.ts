import { spawn, spawnSync, Subprocess } from 'bun'
import { join, resolve } from 'path'
import { readFileSync } from 'fs'
import { createWalletClient, defineChain, Hex, http, publicActions } from 'viem'
import { privateKeyToAccount } from 'viem/accounts'

export function getByteCode(name: string, evm: boolean): Hex {
	const bytecode = evm ? readFileSync(`evm/${name}.bin`) : readFileSync(`pvm/${name}.polkavm`)
	return `0x${Buffer.from(bytecode).toString('hex')}`
}

export type JsonRpcError = {
	code: number
	message: string
	data: Hex
}

export function killProcessOnPort(port: number) {
	// Check which process is using the specified port
	const result = spawnSync(['lsof', '-ti', `:${port}`])
	const output = result.stdout.toString().trim()

	if (output) {
		console.log(`Port ${port} is in use. Killing process...`)
		const pids = output.split('\n')

		// Kill each process using the port
		for (const pid of pids) {
			spawnSync(['kill', '-9', pid])
			console.log(`Killed process with PID: ${pid}`)
		}
	}
}

export let jsonRpcErrors: JsonRpcError[] = []
export async function createEnv(name: 'geth' | 'kitchensink') {
	const gethPort = process.env.GETH_PORT || '8546'
	const kitchensinkPort = process.env.KITCHENSINK_PORT || '8545'
	const url = `http://localhost:${name == 'geth' ? gethPort : kitchensinkPort}`
	const chain = defineChain({
		id: name == 'geth' ? 1337 : 420420420,
		name,
		nativeCurrency: {
			name: 'Westie',
			symbol: 'WST',
			decimals: 18,
		},
		rpcUrls: {
			default: {
				http: [url],
			},
		},
		testnet: true,
	})

	const transport = http(url, {
		onFetchResponse: async (response) => {
			const raw = await response.clone().json()
			if (raw.error) {
				jsonRpcErrors.push(raw.error as JsonRpcError)
			}
		},
	})

	const wallet = createWalletClient({
		transport,
		chain,
	})

	const [account] = await wallet.getAddresses()
	const serverWallet = createWalletClient({
		account,
		transport,
		chain,
	}).extend(publicActions)

	const accountWallet = createWalletClient({
		account: privateKeyToAccount(
			'0xa872f6cbd25a0e04a08b1e21098017a9e6194d101d75e13111f71410c59cd57f'
		),
		transport,
		chain,
	}).extend(publicActions)

	return { serverWallet, accountWallet, evm: name == 'geth' }
}

// wait for http request to return 200
export function waitForHealth(url: string) {
	return new Promise<void>((resolve, reject) => {
		const start = Date.now()
		const interval = setInterval(() => {
			fetch(url)
				.then((res) => {
					if (res.status === 200) {
						clearInterval(interval)
						resolve()
					}
				})
				.catch(() => {
					const elapsed = Date.now() - start
					if (elapsed > 30_000) {
						clearInterval(interval)
						reject(new Error('hit timeout'))
					}
				})
		}, 1000)
	})
}

export const procs: Subprocess[] = []
const polkadotSdkPath = resolve(__dirname, '../../../../../../..')
if (!process.env.USE_LIVE_SERVERS) {
	procs.push(
		// Run geth on port 8546
		//
		(() => {
			killProcessOnPort(8546)
			return spawn(
				'geth --http --http.api web3,eth,debug,personal,net --http.port 8546 --dev --verbosity 0'.split(
					' '
				),
				{ stdout: Bun.file('/tmp/geth.out.log'), stderr: Bun.file('/tmp/geth.err.log') }
			)
		})(),
		//Run the substate node
		(() => {
			killProcessOnPort(9944)
			return spawn(
				[
					'./target/debug/substrate-node',
					'--dev',
					'-l=error,evm=debug,sc_rpc_server=info,runtime::revive=debug',
				],
				{
					stdout: Bun.file('/tmp/kitchensink.out.log'),
					stderr: Bun.file('/tmp/kitchensink.err.log'),
					cwd: polkadotSdkPath,
				}
			)
		})(),
		// Run eth-rpc on 8545
		await (async () => {
			killProcessOnPort(8545)
			const proc = spawn(
				[
					'./target/debug/eth-rpc',
					'--dev',
					'--node-rpc-url=ws://localhost:9944',
					'-l=rpc-metrics=debug,eth-rpc=debug',
				],
				{
					stdout: Bun.file('/tmp/eth-rpc.out.log'),
					stderr: Bun.file('/tmp/eth-rpc.err.log'),
					cwd: polkadotSdkPath,
				}
			)
			await waitForHealth('http://localhost:8545/health').catch()
			return proc
		})()
	)
}
