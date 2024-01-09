package api

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/snowfork/snowbridge/relayer/relays/beacon/config"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/state"

	"github.com/ethereum/go-ethereum/common"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/header/syncer/util"
)

const (
	ConstructRequestErrorMessage = "construct header request"
	DoHTTPRequestErrorMessage    = "do http request"
	HTTPStatusNotOKErrorMessage  = "http status not ok"
	ReadResponseBodyErrorMessage = "read response body"
	UnmarshalBodyErrorMessage    = "unmarshal body"
)

type BeaconAPI interface {
	GetHeader(blockRoot common.Hash) (BeaconHeader, error)
	GetHeaderBySlot(slot uint64) (BeaconHeader, error)
	GetSyncCommitteePeriodUpdate(from uint64) (SyncCommitteePeriodUpdateResponse, error)
	GetBeaconBlock(slot uint64) (BeaconBlockResponse, error)
	GetInitialSync(blockRoot string) (BootstrapResponse, error)
	GetFinalizedCheckpoint() (FinalizedCheckpoint, error)
	GetGenesis() (Genesis, error)
	GetLatestFinalizedUpdate() (LatestFinalisedUpdateResponse, error)
	GetBootstrap(blockRoot common.Hash) (Bootstrap, error)
}

var (
	ErrNotFound                        = errors.New("not found")
	ErrSyncCommitteeUpdateNotAvailable = errors.New("no sync committee update available")
)

type BeaconClient struct {
	httpClient   http.Client
	endpoint     string
	activeSpec   config.ActiveSpec
	slotsInEpoch uint64
}

func NewBeaconClient(endpoint string, activeSpec config.ActiveSpec, slotsInEpoch uint64) *BeaconClient {
	return &BeaconClient{
		http.Client{},
		endpoint,
		activeSpec,
		slotsInEpoch,
	}
}

func (b *BeaconClient) GetBootstrap(blockRoot common.Hash) (BootstrapResponse, error) {
	req, err := http.NewRequest(http.MethodGet, fmt.Sprintf("%s/eth/v1/beacon/light_client/bootstrap/%s", b.endpoint, blockRoot), nil)
	if err != nil {
		return BootstrapResponse{}, fmt.Errorf("%s: %w", ConstructRequestErrorMessage, err)
	}

	req.Header.Set("accept", "application/json")
	res, err := b.httpClient.Do(req)
	if err != nil {
		return BootstrapResponse{}, fmt.Errorf("%s: %w", DoHTTPRequestErrorMessage, err)
	}

	if res.StatusCode != http.StatusOK {
		return BootstrapResponse{}, fmt.Errorf("%s: %d", HTTPStatusNotOKErrorMessage, res.StatusCode)
	}

	bodyBytes, err := io.ReadAll(res.Body)
	if err != nil {
		return BootstrapResponse{}, fmt.Errorf("%s: %w", ReadResponseBodyErrorMessage, err)
	}

	var response BootstrapResponse
	err = json.Unmarshal(bodyBytes, &response)
	if err != nil {
		return BootstrapResponse{}, fmt.Errorf("%s: %w", UnmarshalBodyErrorMessage, err)
	}

	return response, nil
}

func (b *BeaconClient) GetGenesis() (Genesis, error) {
	req, err := http.NewRequest(http.MethodGet, fmt.Sprintf("%s/eth/v1/beacon/genesis", b.endpoint), nil)
	if err != nil {
		return Genesis{}, fmt.Errorf("%s: %w", ConstructRequestErrorMessage, err)
	}

	req.Header.Set("accept", "application/json")
	res, err := b.httpClient.Do(req)
	if err != nil {
		return Genesis{}, fmt.Errorf("%s: %w", DoHTTPRequestErrorMessage, err)
	}

	if res.StatusCode != http.StatusOK {
		return Genesis{}, fmt.Errorf("%s: %d", HTTPStatusNotOKErrorMessage, res.StatusCode)
	}

	bodyBytes, err := io.ReadAll(res.Body)
	if err != nil {
		return Genesis{}, fmt.Errorf("%s: %w", ReadResponseBodyErrorMessage, err)
	}

	var response GenesisResponse
	err = json.Unmarshal(bodyBytes, &response)
	if err != nil {
		return Genesis{}, fmt.Errorf("%s: %w", UnmarshalBodyErrorMessage, err)
	}

	time, err := util.ToUint64(response.Data.Time)
	if err != nil {
		return Genesis{}, fmt.Errorf("convert genesis time string to uint64: %w", err)
	}

	return Genesis{
		ValidatorsRoot: common.HexToHash(response.Data.GenesisValidatorsRoot),
		Time:           time,
	}, nil
}

func (b *BeaconClient) GetFinalizedCheckpoint() (FinalizedCheckpoint, error) {
	req, err := http.NewRequest(http.MethodGet, fmt.Sprintf("%s/eth/v1/beacon/states/head/finality_checkpoints", b.endpoint), nil)
	if err != nil {
		return FinalizedCheckpoint{}, fmt.Errorf("%s: %w", ConstructRequestErrorMessage, err)
	}

	req.Header.Set("accept", "application/json")
	res, err := b.httpClient.Do(req)
	if err != nil {
		return FinalizedCheckpoint{}, fmt.Errorf("%s: %w", DoHTTPRequestErrorMessage, err)
	}

	if res.StatusCode != http.StatusOK {
		return FinalizedCheckpoint{}, fmt.Errorf("%s: %d", HTTPStatusNotOKErrorMessage, res.StatusCode)
	}

	bodyBytes, err := io.ReadAll(res.Body)
	if err != nil {
		return FinalizedCheckpoint{}, fmt.Errorf("%s: %d", ReadResponseBodyErrorMessage, res.StatusCode)
	}

	var response FinalizedCheckpointResponse
	err = json.Unmarshal(bodyBytes, &response)
	if err != nil {
		return FinalizedCheckpoint{}, fmt.Errorf("%s: %w", UnmarshalBodyErrorMessage, err)
	}

	return FinalizedCheckpoint{
		FinalizedBlockRoot: common.HexToHash(response.Data.Finalized.Root),
	}, nil
}

func (b *BeaconClient) GetHeaderBySlot(slot uint64) (BeaconHeader, error) {
	req, err := http.NewRequest(http.MethodGet, fmt.Sprintf("%s/eth/v1/beacon/headers/%d", b.endpoint, slot), nil)
	if err != nil {
		return BeaconHeader{}, fmt.Errorf("%s: %w", ConstructRequestErrorMessage, err)
	}

	req.Header.Set("accept", "application/json")
	res, err := b.httpClient.Do(req)
	if err != nil {
		return BeaconHeader{}, fmt.Errorf("%s: %w", DoHTTPRequestErrorMessage, err)
	}

	if res.StatusCode != http.StatusOK {
		if res.StatusCode == 404 {
			return BeaconHeader{}, ErrNotFound
		}

		return BeaconHeader{}, fmt.Errorf("%s: %d", HTTPStatusNotOKErrorMessage, res.StatusCode)
	}

	bodyBytes, err := io.ReadAll(res.Body)
	if err != nil {
		return BeaconHeader{}, fmt.Errorf("%s: %w", ReadResponseBodyErrorMessage, err)
	}

	var response BeaconHeaderResponse

	err = json.Unmarshal(bodyBytes, &response)
	if err != nil {
		return BeaconHeader{}, fmt.Errorf("%s: %w", UnmarshalBodyErrorMessage, err)
	}

	slotFromResponse, err := strconv.ParseUint(response.Data.Header.Message.Slot, 10, 64)
	if err != nil {
		return BeaconHeader{}, fmt.Errorf("parse slot as int: %w", err)
	}

	proposerIndex, err := strconv.ParseUint(response.Data.Header.Message.ProposerIndex, 10, 64)
	if err != nil {
		return BeaconHeader{}, fmt.Errorf("parse proposerIndex as int: %w", err)
	}

	return BeaconHeader{
		Slot:          slotFromResponse,
		ProposerIndex: proposerIndex,
		ParentRoot:    common.HexToHash(response.Data.Header.Message.ParentRoot),
		StateRoot:     common.HexToHash(response.Data.Header.Message.StateRoot),
		BodyRoot:      common.HexToHash(response.Data.Header.Message.BodyRoot),
	}, nil
}

func (b *BeaconClient) GetHeader(blockRoot common.Hash) (BeaconHeader, error) {
	req, err := http.NewRequest(http.MethodGet, fmt.Sprintf("%s/eth/v1/beacon/headers/%s", b.endpoint, blockRoot), nil)
	if err != nil {
		return BeaconHeader{}, fmt.Errorf("%s: %w", ConstructRequestErrorMessage, err)
	}

	req.Header.Set("accept", "application/json")
	res, err := b.httpClient.Do(req)
	if err != nil {
		return BeaconHeader{}, fmt.Errorf("%s: %w", DoHTTPRequestErrorMessage, err)
	}

	if res.StatusCode != http.StatusOK {
		return BeaconHeader{}, fmt.Errorf("%s: %d", HTTPStatusNotOKErrorMessage, res.StatusCode)
	}

	bodyBytes, err := io.ReadAll(res.Body)
	if err != nil {
		return BeaconHeader{}, fmt.Errorf("%s: %w", ReadResponseBodyErrorMessage, err)
	}

	var response BeaconHeaderResponse

	err = json.Unmarshal(bodyBytes, &response)
	if err != nil {
		return BeaconHeader{}, fmt.Errorf("%s: %w", UnmarshalBodyErrorMessage, err)
	}

	slotScale, err := strconv.ParseUint(response.Data.Header.Message.Slot, 10, 64)
	if err != nil {
		return BeaconHeader{}, fmt.Errorf("parse slot as int: %w", err)
	}

	proposerIndex, err := strconv.ParseUint(response.Data.Header.Message.ProposerIndex, 10, 64)
	if err != nil {
		return BeaconHeader{}, fmt.Errorf("parse proposerIndex as int: %w", err)
	}

	return BeaconHeader{
		Slot:          slotScale,
		ProposerIndex: proposerIndex,
		ParentRoot:    common.HexToHash(response.Data.Header.Message.ParentRoot),
		StateRoot:     common.HexToHash(response.Data.Header.Message.StateRoot),
		BodyRoot:      common.HexToHash(response.Data.Header.Message.BodyRoot),
	}, nil
}

func (b *BeaconClient) GetBeaconBlock(blockID common.Hash) (state.BeaconBlock, error) {
	var beaconBlock state.BeaconBlock

	req, err := http.NewRequest(http.MethodGet, fmt.Sprintf("%s/eth/v2/beacon/blocks/%s", b.endpoint, blockID), nil)
	if err != nil {
		return beaconBlock, fmt.Errorf("%s: %w", ConstructRequestErrorMessage, err)
	}

	req.Header.Add("Accept", "application/json")
	res, err := b.httpClient.Do(req)
	if err != nil {
		return beaconBlock, fmt.Errorf("%s: %w", DoHTTPRequestErrorMessage, err)
	}

	if res.StatusCode != http.StatusOK {
		return beaconBlock, fmt.Errorf("%s: %d", HTTPStatusNotOKErrorMessage, res.StatusCode)
	}

	blockResponse := BeaconBlockResponse{}

	bodyBytes, err := io.ReadAll(res.Body)
	if err != nil {
		return beaconBlock, fmt.Errorf("%s: %w", ReadResponseBodyErrorMessage, err)
	}

	err = json.Unmarshal(bodyBytes, &blockResponse)
	if err != nil {
		return beaconBlock, fmt.Errorf("%s: %w", UnmarshalBodyErrorMessage, err)
	}

	data := blockResponse.Data.Message
	slot, err := util.ToUint64(data.Slot)
	if err != nil {
		return nil, err
	}

	ssz, err := blockResponse.ToFastSSZ(b.activeSpec, slot/b.slotsInEpoch)
	if err != nil {
		return nil, err
	}

	return ssz, nil
}

func (b *BeaconClient) GetBeaconBlockBySlot(slot uint64) (BeaconBlockResponse, error) {
	req, err := http.NewRequest(http.MethodGet, fmt.Sprintf("%s/eth/v2/beacon/blocks/%d", b.endpoint, slot), nil)
	if err != nil {
		return BeaconBlockResponse{}, fmt.Errorf("%s: %w", ConstructRequestErrorMessage, err)
	}

	req.Header.Set("accept", "application/json")
	res, err := b.httpClient.Do(req)
	if err != nil {
		return BeaconBlockResponse{}, fmt.Errorf("%s: %w", DoHTTPRequestErrorMessage, err)
	}

	if res.StatusCode != http.StatusOK {
		if res.StatusCode == 404 {
			return BeaconBlockResponse{}, ErrNotFound
		}

		return BeaconBlockResponse{}, fmt.Errorf("%s: %d", HTTPStatusNotOKErrorMessage, res.StatusCode)
	}

	bodyBytes, err := io.ReadAll(res.Body)
	if err != nil {
		return BeaconBlockResponse{}, fmt.Errorf("%s: %w", ReadResponseBodyErrorMessage, err)
	}

	var response BeaconBlockResponse

	err = json.Unmarshal(bodyBytes, &response)
	if err != nil {
		return BeaconBlockResponse{}, fmt.Errorf("%s: %w", UnmarshalBodyErrorMessage, err)
	}

	return response, nil
}

func (b *BeaconClient) GetBeaconBlockRoot(slot uint64) (common.Hash, error) {
	req, err := http.NewRequest(http.MethodGet, fmt.Sprintf("%s/eth/v1/beacon/blocks/%d/root", b.endpoint, slot), nil)
	if err != nil {
		return common.Hash{}, fmt.Errorf("%s: %w", ConstructRequestErrorMessage, err)
	}

	req.Header.Set("accept", "application/json")
	res, err := b.httpClient.Do(req)
	if err != nil {
		return common.Hash{}, fmt.Errorf("%s: %w", DoHTTPRequestErrorMessage, err)
	}

	if res.StatusCode != http.StatusOK {
		if res.StatusCode == 404 {
			return common.Hash{}, ErrNotFound
		}

		return common.Hash{}, fmt.Errorf("fetch beacon block %d: %s", res.StatusCode, HTTPStatusNotOKErrorMessage)
	}

	bodyBytes, err := io.ReadAll(res.Body)
	if err != nil {
		return common.Hash{}, fmt.Errorf("%s: %w", ReadResponseBodyErrorMessage, err)
	}

	var response struct {
		Data struct {
			Root string `json:"root"`
		} `json:"data"`
	}

	err = json.Unmarshal(bodyBytes, &response)
	if err != nil {
		return common.Hash{}, fmt.Errorf("%s: %w", UnmarshalBodyErrorMessage, err)
	}

	return common.HexToHash(response.Data.Root), nil
}

func (b *BeaconClient) GetSyncCommitteePeriodUpdate(from uint64) (SyncCommitteePeriodUpdateResponse, error) {
	req, err := http.NewRequest(http.MethodGet, fmt.Sprintf("%s/eth/v1/beacon/light_client/updates?start_period=%d&count=1", b.endpoint, from), nil)
	if err != nil {
		return SyncCommitteePeriodUpdateResponse{}, fmt.Errorf("%s: %w", ConstructRequestErrorMessage, err)
	}

	req.Header.Set("accept", "application/json")
	res, err := b.httpClient.Do(req)
	if err != nil {
		return SyncCommitteePeriodUpdateResponse{}, fmt.Errorf("%s: %w", DoHTTPRequestErrorMessage, err)
	}

	if res.StatusCode != http.StatusOK {
		bodyBytes, err := io.ReadAll(res.Body)
		if err != nil {
			return SyncCommitteePeriodUpdateResponse{}, fmt.Errorf("%s: %w", HTTPStatusNotOKErrorMessage, err)
		}

		var response ErrorMessage

		err = json.Unmarshal(bodyBytes, &response)
		if err != nil {
			return SyncCommitteePeriodUpdateResponse{}, fmt.Errorf("%s: %w", HTTPStatusNotOKErrorMessage, err)
		}

		if strings.Contains(response.Message, "No partialUpdate available") {
			return SyncCommitteePeriodUpdateResponse{}, ErrSyncCommitteeUpdateNotAvailable
		}

		return SyncCommitteePeriodUpdateResponse{}, fmt.Errorf("%s :%d", HTTPStatusNotOKErrorMessage, res.StatusCode)
	}

	bodyBytes, err := io.ReadAll(res.Body)
	if err != nil {
		return SyncCommitteePeriodUpdateResponse{}, fmt.Errorf("%s: %w", ReadResponseBodyErrorMessage, err)
	}

	var response []SyncCommitteePeriodUpdateResponse

	err = json.Unmarshal(bodyBytes, &response)
	if err != nil {
		return SyncCommitteePeriodUpdateResponse{}, fmt.Errorf("%s: %w", UnmarshalBodyErrorMessage, err)
	}

	if len(response) == 0 {
		return SyncCommitteePeriodUpdateResponse{}, ErrNotFound
	}

	return response[0], nil
}

func (b *BeaconClient) GetLatestFinalizedUpdate() (LatestFinalisedUpdateResponse, error) {
	req, err := http.NewRequest(http.MethodGet, fmt.Sprintf("%s/eth/v1/beacon/light_client/finality_update", b.endpoint), nil)
	if err != nil {
		return LatestFinalisedUpdateResponse{}, fmt.Errorf("%s: %w", ConstructRequestErrorMessage, err)
	}

	req.Header.Set("accept", "application/json")
	res, err := b.httpClient.Do(req)
	if err != nil {
		return LatestFinalisedUpdateResponse{}, fmt.Errorf("%s: %w", DoHTTPRequestErrorMessage, err)
	}

	if res.StatusCode != http.StatusOK {
		return LatestFinalisedUpdateResponse{}, fmt.Errorf("%s: %d", DoHTTPRequestErrorMessage, res.StatusCode)
	}

	bodyBytes, err := io.ReadAll(res.Body)
	if err != nil {
		return LatestFinalisedUpdateResponse{}, fmt.Errorf("%s: %w", ReadResponseBodyErrorMessage, err)
	}

	var response LatestFinalisedUpdateResponse

	err = json.Unmarshal(bodyBytes, &response)
	if err != nil {
		return LatestFinalisedUpdateResponse{}, fmt.Errorf("%s: %w", UnmarshalBodyErrorMessage, err)
	}

	return response, nil
}

func (b *BeaconClient) DownloadBeaconState(stateIdOrSlot string) (string, error) {
	req, err := http.NewRequest("GET", fmt.Sprintf("%s/eth/v2/debug/beacon/states/%s", b.endpoint, stateIdOrSlot), nil)
	if err != nil {
		return "", err
	}

	req.Header.Add("Accept", "application/octet-stream")
	res, err := b.httpClient.Do(req)
	if err != nil {
		return "", err
	}

	if res.StatusCode != http.StatusOK {
		if res.StatusCode == 404 {
			return "", ErrNotFound
		}

		return "", fmt.Errorf("%s: %d", DoHTTPRequestErrorMessage, res.StatusCode)
	}

	filename := fmt.Sprintf("beacon_state_%d.ssz", time.Now().UnixNano())
	defer res.Body.Close()
	out, err := os.Create(filename)
	if err != nil {
		return "", err
	}

	defer out.Close()
	io.Copy(out, res.Body)

	return filename, nil
}
