import { readFileSync } from 'node:fs'
import { spawn } from 'node:child_process'
import { parseArgs } from 'node:util'
import { createWalletClient, defineChain, Hex, http, parseEther, publicActions } from 'viem'
import { privateKeyToAccount } from 'viem/accounts'

const {
	values: { geth, proxy, westend, endowment, ['private-key']: privateKey },
} = parseArgs({
	args: process.argv.slice(2),
	options: {
		['private-key']: {
			type: 'string',
			short: 'k',
		},
		endowment: {
			type: 'string',
			short: 'e',
		},
		proxy: {
			type: 'boolean',
		},
		geth: {
			type: 'boolean',
		},
		westend: {
			type: 'boolean',
		},
	},
})

if (geth) {
	console.log('Testing with Geth')
	const child = spawn(
		'geth',
		[
			'--http',
			'--http.api',
			'web3,eth,debug,personal,net',
			'--http.port',
			process.env.GETH_PORT ?? '8546',
			'--dev',
			'--verbosity',
			'0',
		],
		{ stdio: 'inherit' }
	)

	process.on('exit', () => child.kill())
	child.unref()
	await new Promise((resolve) => setTimeout(resolve, 500))
}
const rpcUrl = proxy
	? 'http://localhost:8080'
	: westend
		? 'https://westend-asset-hub-eth-rpc.polkadot.io'
		: geth
			? 'http://localhost:8546'
			: 'http://localhost:8545'

export const chain = defineChain({
	id: geth ? 1337 : 420420420,
	name: 'Asset Hub Westend',
	network: 'asset-hub',
	nativeCurrency: {
		name: 'Westie',
		symbol: 'WST',
		decimals: 18,
	},
	rpcUrls: {
		default: {
			http: [rpcUrl],
		},
	},
	testnet: true,
})

const wallet = createWalletClient({
	transport: http(),
	chain,
})
const [account] = await wallet.getAddresses()
export const serverWalletClient = createWalletClient({
	account,
	transport: http(),
	chain,
})

export const walletClient = await (async () => {
	if (privateKey) {
		const account = privateKeyToAccount(`0x${privateKey}`)
		console.log(`Wallet address ${account.address}`)

		const wallet = createWalletClient({
			account,
			transport: http(),
			chain,
		})

		if (endowment) {
			await serverWalletClient.sendTransaction({
				to: account.address,
				value: parseEther(endowment),
			})
			console.log(`Endowed address ${account.address} with: ${endowment}`)
		}

		return wallet.extend(publicActions)
	} else {
		return serverWalletClient.extend(publicActions)
	}
})()

/**
 * Get one of the pre-built contracts
 * @param name - the contract name
 */
export function getByteCode(name: string): Hex {
	const bytecode = geth ? readFileSync(`evm/${name}.bin`) : readFileSync(`pvm/${name}.polkavm`)
	return `0x${Buffer.from(bytecode).toString('hex')}`
}

export function assert(condition: any, message: string): asserts condition {
	if (!condition) {
		throw new Error(message)
	}
}
