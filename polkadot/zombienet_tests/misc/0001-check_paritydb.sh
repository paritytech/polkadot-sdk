# at first check path that works for native provider
DIR=./data/chains/rococo_local_testnet/paritydb/full
if [ ! -d $DIR ] ; then
  # check k8s provider
  DIR=/data/chains/rococo_local_testnet/paritydb/full
fi
ls $DIR 2> /dev/null
