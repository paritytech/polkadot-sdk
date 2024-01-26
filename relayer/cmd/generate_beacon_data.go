package cmd

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"strconv"
	"time"

	"github.com/cbroglie/mustache"
	"github.com/ethereum/go-ethereum/accounts/abi/bind"
	"github.com/ethereum/go-ethereum/common"
	log "github.com/sirupsen/logrus"
	"github.com/snowfork/go-substrate-rpc-client/v4/types"
	"github.com/snowfork/snowbridge/relayer/chain/ethereum"
	"github.com/snowfork/snowbridge/relayer/chain/parachain"
	"github.com/snowfork/snowbridge/relayer/cmd/run/execution"
	"github.com/snowfork/snowbridge/relayer/contracts"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/cache"
	beaconConf "github.com/snowfork/snowbridge/relayer/relays/beacon/config"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/header/syncer"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/header/syncer/api"
	beaconjson "github.com/snowfork/snowbridge/relayer/relays/beacon/header/syncer/json"
	"github.com/snowfork/snowbridge/relayer/relays/beacon/header/syncer/scale"
	executionConf "github.com/snowfork/snowbridge/relayer/relays/execution"
	"github.com/spf13/cobra"
	"github.com/spf13/viper"
)

func generateBeaconDataCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "generate-beacon-data",
		Short: "Generate beacon data.",
		Args:  cobra.ExactArgs(0),
		RunE:  generateBeaconTestFixture,
	}

	cmd.Flags().String("url", "http://127.0.0.1:9596", "Beacon URL")
	cmd.Flags().Bool("wait_until_next_period", true, "Waiting until next period")
	cmd.Flags().Uint32("nonce", 1, "Nonce of the inbound message")
	cmd.Flags().String("test_case", "register_token", "Inbound test case")
	return cmd
}

func generateBeaconCheckpointCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "generate-beacon-checkpoint",
		Short: "Generate beacon checkpoint.",
		Args:  cobra.ExactArgs(0),
		RunE:  generateBeaconCheckpoint,
	}

	cmd.Flags().String("url", "http://127.0.0.1:9596", "Beacon URL")
	cmd.Flags().Bool("export_json", false, "Export Json")

	return cmd
}

func generateExecutionUpdateCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "generate-execution-update",
		Short: "Generate execution update.",
		Args:  cobra.ExactArgs(0),
		RunE:  generateExecutionUpdate,
	}

	cmd.Flags().String("url", "http://127.0.0.1:9596", "Beacon URL")
	cmd.Flags().Uint32("slot", 1, "slot number")
	return cmd
}

type Data struct {
	CheckpointUpdate      beaconjson.CheckPoint
	SyncCommitteeUpdate   beaconjson.Update
	FinalizedHeaderUpdate beaconjson.Update
	HeaderUpdate          beaconjson.HeaderUpdate
	InboundMessageTest    InboundMessageTest
}

type InboundMessageTest struct {
	ExecutionHeader beaconjson.CompactExecutionHeader `json:"execution_header"`
	Message         parachain.MessageJSON             `json:"message"`
}

const (
	pathToBeaconBenchmarkData         = "parachain/pallets/ethereum-client/src/benchmarking/fixtures.rs"
	pathToBenchmarkDataTemplate       = "parachain/templates/benchmarking-fixtures.mustache"
	pathToBeaconTestFixtureFiles      = "parachain/pallets/ethereum-client/tests/fixtures"
	pathToInboundQueueFixtureTemplate = "parachain/templates/%s.mustache"
	pathToInboundQueueFixtureData     = "parachain/pallets/inbound-queue/fixtures/src/%s.rs"
)

// Only print the hex encoded call as output of this command
func generateBeaconCheckpoint(cmd *cobra.Command, _ []string) error {
	err := func() error {
		endpoint, err := cmd.Flags().GetString("url")

		viper.SetConfigFile("web/packages/test/config/beacon-relay.json")

		if err := viper.ReadInConfig(); err != nil {
			return err
		}

		var conf beaconConf.Config
		err = viper.Unmarshal(&conf)
		if err != nil {
			return err
		}

		s := syncer.New(endpoint, conf.Source.Beacon.Spec)

		checkPointScale, err := s.GetCheckpoint()
		if err != nil {
			return fmt.Errorf("get initial sync: %w", err)
		}
		exportJson, err := cmd.Flags().GetBool("export_json")
		if err != nil {
			return err
		}
		if exportJson {
			initialSync := checkPointScale.ToJSON()
			err = writeJSONToFile(initialSync, "dump-initial-checkpoint.json")
			if err != nil {
				return fmt.Errorf("write initial sync to file: %w", err)
			}
		}
		checkPointCallBytes, _ := types.EncodeToBytes(checkPointScale)
		checkPointCallHex := hex.EncodeToString(checkPointCallBytes)
		fmt.Println(checkPointCallHex)
		return nil
	}()
	if err != nil {
		log.WithError(err).Error("error generating beacon checkpoint")
	}

	return nil
}

func generateBeaconTestFixture(cmd *cobra.Command, _ []string) error {
	err := func() error {
		ctx := context.Background()

		endpoint, err := cmd.Flags().GetString("url")
		if err != nil {
			return err
		}

		viper.SetConfigFile("web/packages/test/config/beacon-relay.json")
		if err = viper.ReadInConfig(); err != nil {
			return err
		}

		var conf beaconConf.Config
		err = viper.Unmarshal(&conf)
		if err != nil {
			return err
		}

		log.WithFields(log.Fields{"endpoint": endpoint}).Info("connecting to beacon API")
		s := syncer.New(endpoint, conf.Source.Beacon.Spec)

		viper.SetConfigFile("/tmp/snowbridge/execution-relay-asset-hub.json")

		if err = viper.ReadInConfig(); err != nil {
			return err
		}

		var executionConfig executionConf.Config
		err = viper.Unmarshal(&executionConfig, viper.DecodeHook(execution.HexHookFunc()))
		if err != nil {
			return fmt.Errorf("unable to parse execution relay config: %w", err)
		}

		ethconn := ethereum.NewConnection(&executionConfig.Source.Ethereum, nil)
		err = ethconn.Connect(ctx)
		if err != nil {
			return err
		}

		headerCache, err := ethereum.NewHeaderBlockCache(
			&ethereum.DefaultBlockLoader{Conn: ethconn},
		)
		if err != nil {
			return err
		}

		// generate InitialUpdate
		initialSyncScale, err := s.GetCheckpoint()
		if err != nil {
			return fmt.Errorf("get initial sync: %w", err)
		}
		initialSync := initialSyncScale.ToJSON()
		err = writeJSONToFile(initialSync, fmt.Sprintf("%s/%s", pathToBeaconTestFixtureFiles, "initial-checkpoint.json"))
		if err != nil {
			return err
		}
		initialSyncHeaderSlot := initialSync.Header.Slot
		initialSyncPeriod := s.ComputeSyncPeriodAtSlot(initialSyncHeaderSlot)
		initialEpoch := s.ComputeEpochAtSlot(initialSyncHeaderSlot)
		log.WithFields(log.Fields{
			"epoch":  initialEpoch,
			"period": initialSyncPeriod,
		}).Info("created initial sync file")

		// generate SyncCommitteeUpdate for filling the missing NextSyncCommittee in initial checkpoint
		syncCommitteeUpdateScale, err := s.GetSyncCommitteePeriodUpdate(initialSyncPeriod)
		if err != nil {
			return fmt.Errorf("get sync committee update: %w", err)
		}
		syncCommitteeUpdate := syncCommitteeUpdateScale.Payload.ToJSON()
		err = writeJSONToFile(syncCommitteeUpdate, fmt.Sprintf("%s/%s", pathToBeaconTestFixtureFiles, "sync-committee-update.json"))
		if err != nil {
			return err
		}
		log.Info("created sync committee update file")

		// get inbound message data
		channelID := executionConfig.Source.ChannelID
		address := common.HexToAddress(executionConfig.Source.Contracts.Gateway)
		gatewayContract, err := contracts.NewGateway(address, ethconn.Client())
		if err != nil {
			return err
		}

		nonce, err := cmd.Flags().GetUint32("nonce")
		if err != nil {
			return err
		}

		event, err := getEthereumEvent(ctx, gatewayContract, channelID, nonce)
		if err != nil {
			return err
		}

		receiptTrie, err := headerCache.GetReceiptTrie(ctx, event.Raw.BlockHash)
		if err != nil {
			return err
		}
		inboundMessage, err := ethereum.MakeMessageFromEvent(&event.Raw, receiptTrie)
		if err != nil {
			return err
		}
		messageBlockNumber := event.Raw.BlockNumber

		log.WithFields(log.Fields{
			"message":     inboundMessage,
			"blockHash":   event.Raw.BlockHash.Hex(),
			"blockNumber": messageBlockNumber,
		}).Info("event is at block")

		finalizedUpdateAfterMessage, err := getFinalizedUpdate(*s, messageBlockNumber)
		if err != nil {
			return err
		}

		finalizedHeaderSlot := uint64(finalizedUpdateAfterMessage.Payload.FinalizedHeader.Slot)

		beaconBlock, blockNumber, err := getBeaconBlockContainingExecutionHeader(*s, messageBlockNumber, finalizedHeaderSlot)
		if err != nil {
			return fmt.Errorf("get beacon block containing header: %w", err)
		}

		beaconBlockSlot, err := strconv.ParseUint(beaconBlock.Data.Message.Slot, 10, 64)
		if err != nil {
			return err
		}

		messageJSON := inboundMessage.ToJSON()

		if blockNumber == messageBlockNumber {
			log.WithFields(log.Fields{
				"slot":        beaconBlock.Data.Message.Slot,
				"blockHash":   beaconBlock.Data.Message.Body.ExecutionPayload.BlockHash,
				"blockNumber": blockNumber,
			}).WithError(err).Info("found execution header containing event")
		}

		checkPoint := cache.Proof{
			FinalizedBlockRoot: finalizedUpdateAfterMessage.FinalizedHeaderBlockRoot,
			BlockRootsTree:     finalizedUpdateAfterMessage.BlockRootsTree,
			Slot:               uint64(finalizedUpdateAfterMessage.Payload.FinalizedHeader.Slot),
		}
		headerUpdateScale, err := s.GetNextHeaderUpdateBySlotWithCheckpoint(beaconBlockSlot, &checkPoint)
		if err != nil {
			return fmt.Errorf("get header update: %w", err)
		}
		headerUpdate := headerUpdateScale.ToJSON()

		log.WithField("blockNumber", blockNumber).Info("found beacon block by slot")

		err = writeJSONToFile(headerUpdate, fmt.Sprintf("%s/%s", pathToBeaconTestFixtureFiles, "execution-header-update.json"))
		if err != nil {
			return err
		}
		log.Info("created execution update file")

		compactBeaconHeader := beaconjson.CompactExecutionHeader{
			ParentHash:   headerUpdate.ExecutionHeader.Deneb.ParentHash,
			StateRoot:    headerUpdate.ExecutionHeader.Deneb.StateRoot,
			ReceiptsRoot: headerUpdate.ExecutionHeader.Deneb.ReceiptsRoot,
			BlockNumber:  headerUpdate.ExecutionHeader.Deneb.BlockNumber,
		}

		inboundMessageTest := InboundMessageTest{
			ExecutionHeader: compactBeaconHeader,
			Message:         messageJSON,
		}

		err = writeJSONToFile(inboundMessageTest, fmt.Sprintf("%s/%s", pathToBeaconTestFixtureFiles, "inbound-message.json"))
		if err != nil {
			return err
		}
		log.Info("created inbound message file")

		finalizedUpdate := finalizedUpdateAfterMessage.Payload.ToJSON()
		if finalizedUpdate.AttestedHeader.Slot <= initialSyncHeaderSlot {
			return fmt.Errorf("AttestedHeader slot should be greater than initialSyncHeaderSlot")
		}
		finalizedEpoch := s.ComputeEpochAtSlot(finalizedUpdate.AttestedHeader.Slot)
		if finalizedEpoch <= initialEpoch {
			return fmt.Errorf("epoch in FinalizedUpdate should be greater than initialEpoch")
		}
		finalizedPeriod := s.ComputeSyncPeriodAtSlot(finalizedUpdate.FinalizedHeader.Slot)
		if initialSyncPeriod != finalizedPeriod {
			return fmt.Errorf("initialSyncPeriod should be consistent with finalizedUpdatePeriod")
		}
		err = writeJSONToFile(finalizedUpdate, fmt.Sprintf("%s/%s", pathToBeaconTestFixtureFiles, "finalized-header-update.json"))
		if err != nil {
			return err
		}
		log.WithFields(log.Fields{
			"epoch":  finalizedEpoch,
			"period": finalizedPeriod,
		}).Info("created finalized header update file")

		// Generate benchmark fixture
		log.Info("now updating benchmarking data files")

		// Rust file hexes require the 0x of hashes to be removed
		initialSync.RemoveLeadingZeroHashes()
		syncCommitteeUpdate.RemoveLeadingZeroHashes()
		finalizedUpdate.RemoveLeadingZeroHashes()
		headerUpdate.RemoveLeadingZeroHashes()

		log.WithFields(log.Fields{
			"location": pathToBeaconTestFixtureFiles,
			"template": pathToBenchmarkDataTemplate,
		}).Info("rendering file using mustache")

		inboundMessageTest.Message.RemoveLeadingZeroHashes()
		inboundMessageTest.ExecutionHeader.RemoveLeadingZeroHashes()

		data := Data{
			CheckpointUpdate:      initialSync,
			SyncCommitteeUpdate:   syncCommitteeUpdate,
			FinalizedHeaderUpdate: finalizedUpdate,
			HeaderUpdate:          headerUpdate,
			InboundMessageTest:    inboundMessageTest,
		}

		// writing beacon fixtures
		rendered, err := mustache.RenderFile(pathToBenchmarkDataTemplate, data)
		if err != nil {
			return fmt.Errorf("render beacon benchmark fixture: %w", err)
		}

		log.WithFields(log.Fields{
			"location": pathToBeaconBenchmarkData,
		}).Info("writing result file")

		err = writeBenchmarkDataFile(fmt.Sprintf("%s", pathToBeaconBenchmarkData), rendered)
		if err != nil {
			return err
		}

		// writing inbound queue fixtures
		testCase, err := cmd.Flags().GetString("test_case")
		if err != nil {
			return err
		}
		if testCase != "register_token" || testCase != "send_token" {
			return fmt.Errorf("invalid test case: %s", testCase)
		}
		pathToInboundQueueFixtureTemplate := fmt.Sprintf(pathToInboundQueueFixtureTemplate, testCase)
		pathToInboundQueueFixtureData := fmt.Sprintf(pathToInboundQueueFixtureData, testCase)

		rendered, err = mustache.RenderFile(pathToInboundQueueFixtureTemplate, data)
		if err != nil {
			return fmt.Errorf("render inbound queue benchmark fixture: %w", err)
		}

		log.WithFields(log.Fields{
			"location": pathToInboundQueueFixtureData,
		}).Info("writing result file")

		err = writeBenchmarkDataFile(fmt.Sprintf("%s", pathToInboundQueueFixtureData), rendered)
		if err != nil {
			return err
		}

		// Generate test fixture in next period (require waiting a long time)
		waitUntilNextPeriod, err := cmd.Flags().GetBool("wait_until_next_period")
		if waitUntilNextPeriod {
			log.Info("waiting finalized_update in next period (5 hours later), be patient and wait...")
			for {
				nextFinalizedUpdateScale, err := s.GetFinalizedUpdate()
				if err != nil {
					return fmt.Errorf("get next finalized header update: %w", err)
				}
				nextFinalizedUpdate := nextFinalizedUpdateScale.Payload.ToJSON()
				nextFinalizedUpdatePeriod := s.ComputeSyncPeriodAtSlot(nextFinalizedUpdate.FinalizedHeader.Slot)
				if initialSyncPeriod+1 == nextFinalizedUpdatePeriod {
					err := writeJSONToFile(nextFinalizedUpdate, fmt.Sprintf("%s/%s", pathToBeaconTestFixtureFiles, "next-finalized-header-update.json"))
					if err != nil {
						return err
					}
					log.Info("created next finalized header update file")

					// generate nextSyncCommitteeUpdate
					nextSyncCommitteeUpdateScale, err := s.GetSyncCommitteePeriodUpdate(initialSyncPeriod + 1)
					if err != nil {
						return fmt.Errorf("get sync committee update: %w", err)
					}
					nextSyncCommitteeUpdate := nextSyncCommitteeUpdateScale.Payload.ToJSON()
					err = writeJSONToFile(nextSyncCommitteeUpdate, fmt.Sprintf("%s/%s", pathToBeaconTestFixtureFiles, "next-sync-committee-update.json"))
					if err != nil {
						return err
					}
					log.Info("created next sync committee update file")

					break
				} else {
					log.WithField("slot", nextFinalizedUpdate.FinalizedHeader.Slot).Info("wait 5 minutes for next sync committee period")
					time.Sleep(time.Minute * 5)
				}
			}
		}

		log.Info("done")

		return nil
	}()
	if err != nil {
		log.WithError(err).Error("error generating beacon data")
	}

	return nil
}

func writeJSONToFile(data interface{}, path string) error {
	file, _ := json.MarshalIndent(data, "", "  ")

	f, err := os.OpenFile(path, os.O_RDWR|os.O_CREATE|os.O_TRUNC, 0755)

	if err != nil {
		return fmt.Errorf("create file: %w", err)
	}

	defer f.Close()

	_, err = f.Write(file)

	if err != nil {
		return fmt.Errorf("write to file: %w", err)
	}

	return nil
}

func writeBenchmarkDataFile(path string, fileContents string) error {
	f, err := os.OpenFile(path, os.O_RDWR|os.O_CREATE|os.O_TRUNC, 0755)

	if err != nil {
		return fmt.Errorf("create file: %w", err)
	}

	defer f.Close()

	_, err = f.Write([]byte(fileContents))

	if err != nil {
		return fmt.Errorf("write to file: %w", err)
	}

	return nil
}

func generateExecutionUpdate(cmd *cobra.Command, _ []string) error {
	err := func() error {
		endpoint, _ := cmd.Flags().GetString("url")
		beaconSlot, _ := cmd.Flags().GetUint32("slot")

		viper.SetConfigFile("web/packages/test/config/beacon-relay.json")
		if err := viper.ReadInConfig(); err != nil {
			return err
		}
		var conf beaconConf.Config
		err := viper.Unmarshal(&conf)
		if err != nil {
			return err
		}
		specSettings := conf.Source.Beacon.Spec
		log.WithFields(log.Fields{"endpoint": endpoint}).Info("connecting to beacon API")

		// generate executionUpdate
		s := syncer.New(endpoint, specSettings)
		blockRoot, err := s.Client.GetBeaconBlockRoot(uint64(beaconSlot))
		if err != nil {
			return fmt.Errorf("fetch block: %w", err)
		}
		headerUpdateScale, err := s.GetHeaderUpdate(blockRoot, nil)
		if err != nil {
			return fmt.Errorf("get header update: %w", err)
		}
		headerUpdate := headerUpdateScale.ToJSON()
		err = writeJSONToFile(headerUpdate, "tmp/snowbridge/execution-header-update.json")
		if err != nil {
			return err
		}
		log.Info("created execution update file")

		return nil
	}()
	if err != nil {
		log.WithError(err).Error("error generating beacon execution update")
	}

	return nil
}

func generateInboundTestFixture(ctx context.Context, beaconEndpoint string) error {

	return nil
}

func getEthereumEvent(ctx context.Context, gatewayContract *contracts.Gateway, channelID executionConf.ChannelID, nonce uint32) (*contracts.GatewayOutboundMessageAccepted, error) {
	maxBlockNumber := uint64(10000)

	opts := bind.FilterOpts{
		Start:   1,
		End:     &maxBlockNumber,
		Context: ctx,
	}

	var event *contracts.GatewayOutboundMessageAccepted

	for event == nil {
		log.Info("looking for Ethereum event")

		iter, err := gatewayContract.FilterOutboundMessageAccepted(&opts, [][32]byte{channelID}, [][32]byte{})
		if err != nil {
			return nil, err
		}

		for {
			more := iter.Next()
			if !more {
				err = iter.Error()
				if err != nil {
					return nil, err
				}
				break
			}
			if iter.Event.Nonce >= uint64(nonce) {
				event = iter.Event
				iter.Close()
				break
			}
		}

		time.Sleep(5 * time.Second)
	}

	log.WithField("event", event).Info("found event")

	return event, nil
}

func getBeaconBlockContainingExecutionHeader(s syncer.Syncer, messageBlockNumber, finalizedSlot uint64) (api.BeaconBlockResponse, uint64, error) {
	// quick check to see if the blocknumber == slotnumber (often the case in the testnet).
	// in that case we found the beacon block containing the execution header quickly and can return
	beaconBlock, err := s.Client.GetBeaconBlockBySlot(messageBlockNumber)
	if err != nil {
		return api.BeaconBlockResponse{}, 0, err
	}
	blockNumber, err := strconv.ParseUint(beaconBlock.Data.Message.Body.ExecutionPayload.BlockNumber, 10, 64)
	if err != nil {
		return api.BeaconBlockResponse{}, 0, err
	}

	// we've got the block, return it
	if blockNumber == messageBlockNumber {
		log.WithField("blockNumber", blockNumber).Info("found beacon block, same slot as block number")
		return beaconBlock, 0, nil
	}

	log.Info("searching for beacon block by execution block number")

	beaconHeaderSlot := finalizedSlot
	log.WithField("beaconHeaderSlot", beaconHeaderSlot).Info("getting beacon block by slot")

	for blockNumber != messageBlockNumber && beaconHeaderSlot > 1 {
		beaconHeaderSlot = beaconHeaderSlot - 1
		log.WithField("beaconHeaderSlot", beaconHeaderSlot).Info("getting beacon block by slot")

		beaconBlock, blockNumber, err = getBeaconBlockAndBlockNumber(s, beaconHeaderSlot)
	}

	return beaconBlock, blockNumber, nil
}

func getBeaconBlockAndBlockNumber(s syncer.Syncer, slot uint64) (api.BeaconBlockResponse, uint64, error) {
	beaconBlock, err := s.Client.GetBeaconBlockBySlot(slot)
	if err != nil {
		return api.BeaconBlockResponse{}, 0, err
	}
	blockNumber, err := strconv.ParseUint(beaconBlock.Data.Message.Body.ExecutionPayload.BlockNumber, 10, 64)
	if err != nil {
		return api.BeaconBlockResponse{}, 0, err
	}

	log.WithField("blockNumber", blockNumber).Info("found beacon block by slot")

	return beaconBlock, blockNumber, nil
}

func getFinalizedUpdate(s syncer.Syncer, eventBlockNumber uint64) (*scale.Update, error) {
	var blockNumber uint64
	var finalizedUpdate scale.Update
	var err error

	for blockNumber < eventBlockNumber {

		finalizedUpdate, err = s.GetFinalizedUpdate()
		if err != nil {
			return nil, err
		}

		finalizedSlot := uint64(finalizedUpdate.Payload.FinalizedHeader.Slot)
		log.WithField("slot", finalizedSlot).Info("found finalized update at slot")

		beaconBlock, err := s.Client.GetBeaconBlockBySlot(finalizedSlot)
		if err != nil {
			return nil, err
		}

		blockNumber, err = strconv.ParseUint(beaconBlock.Data.Message.Body.ExecutionPayload.BlockNumber, 10, 64)
		if err != nil {
			return nil, err
		}

		if blockNumber > eventBlockNumber {
			log.Info("found finalized block after message")
			break
		}
		// wait for finalized header after event
		log.Info("waiting for chain to finalize after message...")
		time.Sleep(20 * time.Second)
	}

	return &finalizedUpdate, nil
}
