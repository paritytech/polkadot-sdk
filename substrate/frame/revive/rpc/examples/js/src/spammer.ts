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

//Run the substate node
console.log('🚀 Start kitchensink...')
killProcessOnPort(9944)
spawn(
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

// Run eth-indexer
console.log('🔍 Start indexer...')
spawn(
	[
		'./target/debug/eth-indexer',
		'--node-rpc-url=ws://localhost:9944',
		'-l=eth-rpc=debug',
		'--database-url ${polkadotSdkPath}/substrate/frame/revive/rpc/tx_hashes.db',
	],
	{
		stdout: Bun.file('/tmp/eth-indexer.out.log'),
		stderr: Bun.file('/tmp/eth-indexer.err.log'),
		cwd: polkadotSdkPath,
	}
)

// Run eth-rpc on 8545
console.log('💻 Start eth-rpc...')
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
await waitForHealth('http://localhost:8545').catch()

const env = await createEnv('kitchensink')
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
			console.log(`${receipt.status ? '✅' : '❌'} - ${hash}`)
		})
		await wait(1_000)
		nonce++
	}
} catch (err) {
	console.error('Failed with error:', err)
}
