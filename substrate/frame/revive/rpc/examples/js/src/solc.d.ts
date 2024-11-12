declare module 'solc' {
	// Basic types for input/output handling
	export interface CompileInput {
		language: string
		sources: {
			[fileName: string]: {
				content: string
			}
		}
		settings?: {
			optimizer?: {
				enabled: boolean
				runs: number
			}
			outputSelection: {
				[fileName: string]: {
					[contractName: string]: string[]
				}
			}
		}
	}

	export interface CompileOutput {
		errors?: Array<{
			component: string
			errorCode: string
			formattedMessage: string
			message: string
			severity: string
			sourceLocation?: {
				file: string
				start: number
				end: number
			}
			type: string
		}>
		sources?: {
			[fileName: string]: {
				id: number
				ast: object
			}
		}
		contracts?: {
			[fileName: string]: {
				[contractName: string]: {
					abi: object[]
					evm: {
						bytecode: {
							object: string
							sourceMap: string
							linkReferences: {
								[fileName: string]: {
									[libraryName: string]: Array<{
										start: number
										length: number
									}>
								}
							}
						}
						deployedBytecode: {
							object: string
							sourceMap: string
							linkReferences: {
								[fileName: string]: {
									[libraryName: string]: Array<{
										start: number
										length: number
									}>
								}
							}
						}
					}
				}
			}
		}
	}

	// Main exported functions
	export function compile(
		input: string | CompileInput,
		options?: { import: (path: string) => { contents: string } }
	): string
}
