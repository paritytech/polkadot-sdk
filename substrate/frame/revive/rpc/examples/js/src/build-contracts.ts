import { compile } from '@parity/revive'
import { format } from 'prettier'
import { parseArgs } from 'node:util'
import solc from 'solc'
import { readFileSync, writeFileSync } from 'fs'
import { join } from 'path'

type CompileInput = Parameters<typeof compile>[0]

const {
	values: { filter },
} = parseArgs({
	args: process.argv.slice(2),
	options: {
		filter: {
			type: 'string',
			short: 'f',
		},
	},
})

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
	{ file: 'PiggyBank.sol', contract: 'PiggyBank', keypath: 'piggyBank' },
	{ file: 'ErrorTester.sol', contract: 'ErrorTester', keypath: 'errorTester' },
].filter(({ keypath }) => !filter || keypath.includes(filter))

for (const { keypath, contract, file } of input) {
	const input = {
		[file]: { content: readFileSync(join('contracts', file), 'utf8') },
	}

	{
		console.log(`Compile with solc ${file}`)
		const out = JSON.parse(evmCompile(input))
		const entry = out.contracts[file][contract]
		writeFileSync(join('evm', `${keypath}.bin`), Buffer.from(entry.evm.bytecode.object, 'hex'))
		writeFileSync(
			join('abi', `${keypath}.ts`),
			await format(`export const abi = ${JSON.stringify(entry.abi, null, 2)} as const`, {
				parser: 'typescript',
			})
		)
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
