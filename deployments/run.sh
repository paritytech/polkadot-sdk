#!/bin/bash

# Script used for running and updating bridge deployments.
#
# To deploy a network you can run this script with the name of the bridge (or multiple bridges) you want to run.
#
# `./run.sh westend-millau rialto-millau`
#
# To update a deployment to use the latest images available from the Docker Hub add the `update`
# argument after the bridge name.
#
# `./run.sh rialto-millau update`
#
# Once you've stopped having fun with your deployment you can take it down with:
#
# `./run.sh rialto-millau stop`
#
# Stopping the bridge will also bring down all networks that it uses. So if you have started multiple bridges
# that are using the same network (like Millau in rialto-millau and westend-millau bridges), then stopping one
# of these bridges will cause the other bridge to break.

set -xeu

# Since the Compose commands are using relative paths we need to `cd` into the `deployments` folder.
cd "$( dirname "${BASH_SOURCE[0]}" )"

function show_help () {
  set +x
  echo " "
  echo Error: $1
  echo " "
  echo "Usage:"
  echo "  ./run.sh rialto-millau [stop|update]            Run Rialto <> Millau Networks & Bridge"
  echo "  ./run.sh rialto-parachain-millau [stop|update]  Run RialtoParachain <> Millau Networks & Bridge"
  echo "  ./run.sh westend-millau [stop|update]           Run Westend -> Millau Networks & Bridge"
  echo "  ./run.sh everything|all [stop|update]           Run all available Networks & Bridges"
  echo " "
  echo "Options:"
  echo "  --no-monitoring                            Disable monitoring"
  echo "  --no-ui                                    Disable UI"
  echo "  --local                                    Use prebuilt local images when starting relay and nodes"
  echo "  --local-substrate-relay                    Use prebuilt local/substrate-realy image when starting relay"
  echo "  --local-rialto                             Use prebuilt local/rialto-bridge-node image when starting nodes"
  echo "  --local-rialto-parachain                   Use prebuilt local/rialto-parachain-collator image when starting nodes"
  echo "  --local-millau                             Use prebuilt local/millau-bridge-node image when starting nodes"
  echo "  --no-grafana-startup-delay                 Start Grafana without any delay (you may see some false alerts during startup)"
  echo " "
  echo "You can start multiple bridges at once by passing several bridge names:"
  echo "  ./run.sh rialto-millau rialto-parachain-millau westend-millau [stop|update]"
  exit 1
}

RIALTO=' -f ./networks/rialto.yml'
RIALTO_PARACHAIN=' -f ./networks/rialto-parachain.yml'
MILLAU=' -f ./networks/millau.yml'

RIALTO_MILLAU='rialto-millau'
RIALTO_PARACHAIN_MILLAU='rialto-parachain-millau'
WESTEND_MILLAU='westend-millau'

MONITORING=' -f ./monitoring/docker-compose.yml'
UI=' -f ./ui/docker-compose.yml'

BRIDGES=()
NETWORKS=''
SUB_COMMAND='start'
for i in "$@"
do
  case $i in
    --no-monitoring)
      MONITORING=" -f ./monitoring/disabled.yml"
      shift
      continue
      ;;
    --no-ui)
      UI=""
      shift
      continue
      ;;
    --local)
      export SUBSTRATE_RELAY_IMAGE=local/substrate-relay
      export RIALTO_BRIDGE_NODE_IMAGE=local/rialto-bridge-node
      export RIALTO_PARACHAIN_COLLATOR_IMAGE=local/rialto-parachain-collator
      export MILLAU_BRIDGE_NODE_IMAGE=local/millau-bridge-node
      export IMMEDIATE_
      shift
      continue
      ;;
    --local-substrate-relay)
      export SUBSTRATE_RELAY_IMAGE=local/substrate-relay
      shift
      continue
      ;;
    --local-rialto)
      export RIALTO_BRIDGE_NODE_IMAGE=local/rialto-bridge-node
      shift
      continue
      ;;
    --local-rialto-parachain)
      export RIALTO_PARACHAIN_COLLATOR_IMAGE=local/rialto-parachain-collator
      shift
      continue
      ;;
    --local-millau)
      export MILLAU_BRIDGE_NODE_IMAGE=local/millau-bridge-node
      shift
      continue
      ;;
    --no-grafana-startup-delay)
      export NO_GRAFANA_STARTUP_DELAY="echo 'No Grafana startup delay'"
      shift
      continue
      ;;
    everything|all)
      BRIDGES=(${RIALTO_MILLAU:-} ${RIALTO_PARACHAIN_MILLAU:-} ${WESTEND_MILLAU:-})
      NETWORKS="${RIALTO:-} ${RIALTO_PARACHAIN:-} ${MILLAU:-}"
      unset RIALTO RIALTO_PARACHAIN MILLAU RIALTO_MILLAU RIALTO_PARACHAIN_MILLAU WESTEND_MILLAU
      shift
      ;;
    rialto-millau)
      BRIDGES+=(${RIALTO_MILLAU:-})
      NETWORKS+="${RIALTO:-} ${MILLAU:-}"
      unset RIALTO MILLAU RIALTO_MILLAU
      shift
      ;;
    rialto-parachain-millau)
      BRIDGES+=(${RIALTO_PARACHAIN_MILLAU:-})
      NETWORKS+="${RIALTO:-} ${RIALTO_PARACHAIN:-} ${MILLAU:-}"
      unset RIALTO RIALTO_PARACHAIN MILLAU RIALTO_PARACHAIN_MILLAU
      shift
      ;;
    westend-millau)
      BRIDGES+=(${WESTEND_MILLAU:-})
      NETWORKS+=${MILLAU:-}
      unset MILLAU WESTEND_MILLAU
      shift
      ;;
    start|stop|update)
      SUB_COMMAND=$i
      shift
      ;;
    *)
      show_help "Unknown option: $i"
      ;;
  esac
done

if [ ${#BRIDGES[@]} -eq 0 ]; then
  show_help "Missing bridge name."
fi

COMPOSE_FILES=$NETWORKS$MONITORING$UI

# Compose looks for .env files in the the current directory by default, we don't want that
COMPOSE_ARGS="--project-directory ."
# Path to env file that we want to use. Compose only accepts single `--env-file` argument,
# so we'll be using the last .env file we'll found.
COMPOSE_ENV_FILE=''

for BRIDGE in "${BRIDGES[@]}"
do
  BRIDGE_PATH="./bridges/$BRIDGE"
  BRIDGE=" -f $BRIDGE_PATH/docker-compose.yml"
  COMPOSE_FILES=$BRIDGE$COMPOSE_FILES

  # Remember .env file to use in docker-compose call
  if [[ -f "$BRIDGE_PATH/.env" ]]; then
    COMPOSE_ENV_FILE=" --env-file $BRIDGE_PATH/.env"
  fi

  # Read and source variables from .env file so we can use them here
  grep -e MATRIX_ACCESS_TOKEN -e WITH_PROXY $BRIDGE_PATH/.env > .env2 && . ./.env2 && rm .env2
  if [ ! -z ${MATRIX_ACCESS_TOKEN+x} ]; then
    sed -i "s/access_token.*/access_token: \"$MATRIX_ACCESS_TOKEN\"/" ./monitoring/grafana-matrix/config.yml
  fi
done

# Final COMPOSE_ARGS
COMPOSE_ARGS="$COMPOSE_ARGS $COMPOSE_ENV_FILE"

# Check the sub-command, perhaps we just mean to stop the network instead of starting it.
if [ "$SUB_COMMAND" == "stop" ]; then

  if [ ! -z ${WITH_PROXY+x} ]; then
    cd ./reverse-proxy
    docker-compose down
    cd -
  fi

  docker-compose $COMPOSE_ARGS $COMPOSE_FILES down

  exit 0
fi

# See if we want to update the docker images before starting the network.
if [ "$SUB_COMMAND" == "update" ]; then

  # Stop the proxy cause otherwise the network can't be stopped
  if [ ! -z ${WITH_PROXY+x} ]; then
    cd ./reverse-proxy
    docker-compose down
    cd -
  fi


  docker-compose $COMPOSE_ARGS $COMPOSE_FILES pull
  docker-compose $COMPOSE_ARGS $COMPOSE_FILES down
  docker-compose $COMPOSE_ARGS $COMPOSE_FILES build
fi

docker-compose $COMPOSE_ARGS $COMPOSE_FILES up -d

# Start the proxy if needed
if [ ! -z ${WITH_PROXY+x} ]; then
  cd ./reverse-proxy
  docker-compose up -d
fi
