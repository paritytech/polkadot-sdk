package json

type ExecutionPayloadHeaderDeneb struct {
	ParentHash      string `json:"parent_hash"`
	FeeRecipient    string `json:"fee_recipient"`
	StateRoot       string `json:"state_root"`
	ReceiptsRoot    string `json:"receipts_root"`
	LogsBloom       string `json:"logs_bloom"`
	PrevRandao      string `json:"prev_randao"`
	BlockNumber     uint64 `json:"block_number"`
	GasLimit        uint64 `json:"gas_limit"`
	GasUsed         uint64 `json:"gas_used"`
	Timestamp       uint64 `json:"timestamp"`
	ExtraData       string `json:"extra_data"`
	BaseFeePerGas   uint64 `json:"base_fee_per_gas"`
	BlockHash       string `json:"block_hash"`
	TransactionRoot string `json:"transactions_root"`
	WithdrawalsRoot string `json:"withdrawals_root"`
	BlobGasUsed     uint64 `json:"blob_gas_used"`
	ExcessBlobGas   uint64 `json:"excess_blob_gas"`
}

func (e *ExecutionPayloadHeaderDeneb) RemoveLeadingZeroHashes() {
	e.ParentHash = removeLeadingZeroHash(e.ParentHash)
	e.FeeRecipient = removeLeadingZeroHash(e.FeeRecipient)
	e.StateRoot = removeLeadingZeroHash(e.StateRoot)
	e.ReceiptsRoot = removeLeadingZeroHash(e.ReceiptsRoot)
	e.LogsBloom = removeLeadingZeroHash(e.LogsBloom)
	e.PrevRandao = removeLeadingZeroHash(e.PrevRandao)
	e.ExtraData = removeLeadingZeroHash(e.ExtraData)
	e.BlockHash = removeLeadingZeroHash(e.BlockHash)
	e.TransactionRoot = removeLeadingZeroHash(e.TransactionRoot)
	e.WithdrawalsRoot = removeLeadingZeroHash(e.WithdrawalsRoot)
}
