import { spawnSync } from 'bun'
import { resolve } from 'path'
import { readFileSync } from 'fs'
import {
	CallParameters,
	createClient,
	createWalletClient,
	defineChain,
	formatTransactionRequest,
	type Hex,
	hexToNumber,
	http,
	publicActions,
} from 'viem'
import { privateKeyToAccount, nonceManager } from 'viem/accounts'

export function getByteCode(name: string, evm: boolean = false): Hex {
	const bytecode = evm ? readFileSync(`evm/${name}.bin`) : readFileSync(`pvm/${name}.polkavm`)
	return `0x${Buffer.from(bytecode).toString('hex')}`
}

export type JsonRpcError = {
	code: number
	message: string
	data: Hex
}

export const polkadotSdkPath = resolve(__dirname, '../../../../../../..')

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
export async function createEnv(name: 'geth' | 'eth-rpc') {
	const gethPort = process.env.GETH_PORT || '8546'
	const ethRpcPort = process.env.ETH_RPC_PORT || '8545'
	const url = `http://localhost:${name == 'geth' ? gethPort : ethRpcPort}`

	let id = await (async () => {
		const resp = await fetch(url, {
			method: 'POST',
			headers: {
				'Content-Type': 'application/json',
			},
			body: JSON.stringify({ jsonrpc: '2.0', method: 'eth_chainId', id: 1 }),
		})
		let { result } = await resp.json()
		return hexToNumber(result)
	})()

	const chain = defineChain({
		id,
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
			'0x5fb92d6e98884f76de468fa3f6278f8807c48bebc13595d45af5bdc4da702133',
			{ nonceManager }
		),
		transport,
		chain,
	}).extend(publicActions)

	const emptyWallet = createWalletClient({
		account: privateKeyToAccount(
			'0x4450c571bae82da0528ecf76fcf7079e12ecc46dc873c9cacb6db8b75ed22f41',
			{ nonceManager }
		),
		transport,
		chain,
	}).extend(publicActions)

	const debugClient = createClient({
		chain,
		transport,
	}).extend((client) => ({
		async traceTransaction(txHash: Hex, tracerConfig: { withLog: boolean }) {
			return client.request({
				method: 'debug_traceTransaction' as any,
				params: [txHash, { tracer: 'callTracer', tracerConfig } as any],
			})
		},
		async traceBlock(blockNumber: bigint, tracerConfig: { withLog: boolean }) {
			return client.request({
				method: 'debug_traceBlockByNumber' as any,
				params: [
					`0x${blockNumber.toString(16)}`,
					{ tracer: 'callTracer', tracerConfig } as any,
				],
			})
		},

		async traceCall(args: CallParameters, tracerConfig: { withLog: boolean }) {
			return client.request({
				method: 'debug_traceCall' as any,
				params: [
					formatTransactionRequest(args),
					'latest',
					{ tracer: 'callTracer', tracerConfig } as any,
				],
			})
		},
	}))

	return { debugClient, emptyWallet, serverWallet, accountWallet, evm: name == 'geth' }
}

export type Env = Awaited<ReturnType<typeof createEnv>>

export function wait(ms: number) {
	return new Promise((resolve) => setTimeout(resolve, ms))
}

export function timeout(ms: number) {
	return new Promise((_resolve, reject) => setTimeout(() => reject(new Error('timeout hit')), ms))
}

// wait for http request to return 200
export function waitForHealth(url: string) {
	return new Promise<void>((resolve, reject) => {
		const start = Date.now()
		const interval = setInterval(async () => {
			try {
				const res = await fetch(url, {
					method: 'POST',
					headers: {
						'content-type': 'application/json',
					},
					body: JSON.stringify({
						jsonrpc: '2.0',
						method: 'eth_syncing',
						params: [],
						id: 1,
					}),
				})

				if (res.status !== 200) {
					return
				}

				clearInterval(interval)
				resolve()
			} catch (_err) {
				const elapsed = Date.now() - start
				if (elapsed > 30_000) {
					clearInterval(interval)
					reject(new Error('hit timeout'))
				}
			}
		}, 1000)
	})
}

export function visit(obj: any, callback: (key: string, value: any) => any): any {
	if (Array.isArray(obj)) {
		return obj.map((item) => visit(item, callback))
	} else if (typeof obj === 'object' && obj !== null) {
		return Object.keys(obj).reduce((acc, key) => {
			acc[key] = visit(callback(key, obj[key]), callback)
			return acc
		}, {} as any)
	} else {
		return obj
	}
}
