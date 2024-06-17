#!/bin/bash -x

if [ "$#" -ne 1 ]; then
  echo "Usage: $0 dir"
  exit 1
fi

MAIN=$1

if [ ! -d $MAIN ]; then
  echo "$MAIN directory does not exists..."
  exit -1
fi

rm $MAIN/start 
rm $MAIN/end 
rm -rf $MAIN/alice
rm -rf $MAIN/bob

rm $MAIN/*.csv
rm $MAIN/*.gnu
rm $MAIN/*.png
# ./draw-propaged-imported-summed.sh propagated-imported $MAIN
# ./draw-validate-transaction.sh validate-transaction $MAIN

