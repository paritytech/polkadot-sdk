//! Run with bun run script-event.ts

import { abi } from '../abi/event.ts'
import { assert, getByteCode, walletClient } from './lib.ts'

const deployHash = await walletClient.deployContract({
	abi,
	bytecode: getByteCode('event'),
})
const deployReceipt = await walletClient.waitForTransactionReceipt({ hash: deployHash })
const contractAddress = deployReceipt.contractAddress
console.log('Contract deployed:', contractAddress)
assert(contractAddress, 'Contract address should be set')

const { request } = await walletClient.simulateContract({
	account: walletClient.account,
	address: contractAddress,
	abi,
	functionName: 'triggerEvent',
})

const hash = await walletClient.writeContract(request)
const receipt = await walletClient.waitForTransactionReceipt({ hash })
console.log(`Receipt: ${receipt.status}`)
console.log(`Logs receipt: ${receipt.status}`)

for (const log of receipt.logs) {
	console.log('Event log:', log)
}
