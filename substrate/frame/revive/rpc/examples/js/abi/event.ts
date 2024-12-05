export const abi = [
	{
		anonymous: false,
		inputs: [
			{
				indexed: true,
				internalType: 'address',
				name: 'sender',
				type: 'address',
			},
			{
				indexed: false,
				internalType: 'uint256',
				name: 'value',
				type: 'uint256',
			},
			{
				indexed: false,
				internalType: 'string',
				name: 'message',
				type: 'string',
			},
		],
		name: 'ExampleEvent',
		type: 'event',
	},
	{
		inputs: [],
		name: 'triggerEvent',
		outputs: [],
		stateMutability: 'nonpayable',
		type: 'function',
	},
] as const
