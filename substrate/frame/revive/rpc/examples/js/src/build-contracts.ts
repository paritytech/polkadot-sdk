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
		writeFileSync(join('evm', `${keypath}.bin`), Buffer.from(entry.evm.bytecode.object, 'hex'))
		writeFileSync(join('abi', `${keypath}.json`), JSON.stringify(entry.abi, null, 2))
	}

	{
		console.log(`Compile with revive ${file}`)
		const out = await compile(input)
		const entry = out.contracts[file][contract]
		writeFileSync(
			join('pvm', `${keypath}.polkavm`),
			Buffer.from(entry.evm.bytecode.object, 'hex')
		)
	}
}
