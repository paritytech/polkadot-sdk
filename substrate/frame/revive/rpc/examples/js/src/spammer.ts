import { spawn } from 'bun'
import {
	createEnv,
	getByteCode,
	killProcessOnPort,
	polkadotSdkPath,
	wait,
	waitForHealth,
} from './util'
import { FlipperAbi } from '../abi/Flipper'

if (process.env.START_SUBSTRATE_NODE) {
	//Run the substate node
	console.log('🚀 Start substrate-node...')
	killProcessOnPort(9944)
	spawn(
		[
			'./target/debug/substrate-node',
			'--dev',
			'-l=error,evm=debug,sc_rpc_server=info,runtime::revive=debug',
		],
		{
			stdout: Bun.file('/tmp/substrate-node.out.log'),
			stderr: Bun.file('/tmp/substrate-node.err.log'),
			cwd: polkadotSdkPath,
		}
	)
}

// Run eth-rpc on 8545
if (process.env.START_ETH_RPC) {
	console.log('🚀 Start eth-rpc...')
	killProcessOnPort(8545)
	spawn(
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
}

await waitForHealth('http://localhost:8545').catch()
const env = await createEnv('eth-rpc')
const wallet = env.accountWallet

console.log('🚀 Deploy flipper...')
const hash = await wallet.deployContract({
	abi: FlipperAbi,
	bytecode: getByteCode('Flipper'),
})

const deployReceipt = await wallet.waitForTransactionReceipt({ hash })
if (!deployReceipt.contractAddress) throw new Error('Contract address should be set')
const flipperAddr = deployReceipt.contractAddress

let nonce = await wallet.getTransactionCount(wallet.account)

console.log('🔄 Starting loop...')
console.log('Starting nonce:', nonce)
try {
	while (true) {
		console.log(`Call flip (nonce: ${nonce})...`)
		const { request } = await wallet.simulateContract({
			account: wallet.account,
			address: flipperAddr,
			abi: FlipperAbi,
			functionName: 'flip',
			nonce,
		})

		const hash = await wallet.writeContract(request)
		console.time(hash)
		wallet.waitForTransactionReceipt({ hash }).then((receipt) => {
			console.timeEnd(hash)
			console.log('-----------------------------------')
			console.log(`status: ${receipt.status ? '✅' : '❌'}`)
			console.log(`block: ${receipt.blockNumber} - hash: ${receipt.blockHash}`)
			console.log(`tx: ${hash}`)
			console.log('-----------------------------------')
		})
		await wait(1_000)
		nonce++
	}
} catch (err) {
	console.error('Failed with error:', err)
}
