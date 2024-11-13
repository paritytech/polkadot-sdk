import { compile } from '@parity/revive'
import solc from 'solc'
import { readFileSync, writeFileSync } from 'fs'
import { join } from 'path'

type CompileInput = Parameters<typeof compile>[0]
type CompileOutput = Awaited<ReturnType<typeof compile>>
type Abi = CompileOutput['contracts'][string][string]['abi']

function evmCompile(sources: CompileInput) {
	const input = {
		language: 'Solidity',
		sources,
		settings: {
			outputSelection: {
				'*': {
					'*': ['*'],
				},
			},
		},
	}

	return solc.compile(JSON.stringify(input))
}

console.log('Compiling contracts...')

let pvmContracts: Map<string, { abi: Abi; bytecode: string }> = new Map()
let evmContracts: Map<string, { abi: Abi; bytecode: string }> = new Map()
const input = [
	{ file: 'Event.sol', contract: 'EventExample', keypath: 'event' },
	{ file: 'Revert.sol', contract: 'RevertExample', keypath: 'revert' },
	{ file: 'PiggyBank.sol', contract: 'PiggyBank', keypath: 'piggyBank' },
]

for (const { keypath, contract, file } of input) {
	const input = {
		[file]: { content: readFileSync(join('contracts', file), 'utf8') },
	}

	{
		console.log(`Compile with solc ${file}`)
		const out = JSON.parse(evmCompile(input))
		const entry = out.contracts[file][contract]
		evmContracts.set(keypath, { abi: entry.abi, bytecode: entry.evm.bytecode.object })
	}

	{
		console.log(`Compile with revive ${file}`)
		const out = await compile(input)
		const entry = out.contracts[file][contract]
		pvmContracts.set(keypath, { abi: entry.abi, bytecode: entry.evm.bytecode.object })
	}
}

writeFileSync('pvm-contracts.json', JSON.stringify(Object.fromEntries(pvmContracts), null, 2))
writeFileSync('evm-contracts.json', JSON.stringify(Object.fromEntries(evmContracts), null, 2))
