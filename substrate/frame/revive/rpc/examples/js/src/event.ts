//! Run with bun run script-event.ts
import { call, getContract, deploy } from './lib.ts'

try {
	const { abi, bytecode } = getContract('event')
	const address = await deploy(bytecode, abi)
	const receipt = await call('triggerEvent', address, abi)
	if (receipt) {
		for (const log of receipt.logs) {
			console.log('Event log:', JSON.stringify(log, null, 2))
		}
	}
} catch (err) {
	console.error(err)
}
