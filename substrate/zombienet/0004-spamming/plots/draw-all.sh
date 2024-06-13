#!/bin/bash -x

MAIN=$1

if [ ! -d $MAIN ]; then
  echo "$MAIN directory does not exists..."
  exit -1
fi
if [ ! -f $MAIN/alice.log ]; then
  echo "alice.log in $MAIN directory does not exists..."
  exit -1
fi
if [ ! -f $MAIN/bob.log ]; then
  echo "bob.log in $MAIN directory does not exists..."
  exit -1
fi

if [ ! -f $MAIN/start ]; then
  grep "DEBUG.*runtime_api.validate_transaction" $MAIN/alice.log | head -n 1 | cut -f2 -d' ' | cut -f1 -d'.' > $MAIN/start
fi

if [ ! -f $MAIN/end ]; then
  grep "DEBUG.*runtime_api.validate_transaction" $MAIN/alice.log | tail -n 1 | cut -f2 -d' ' | cut -f1 -d'.' > $MAIN/end
fi



if [ ! -d $MAIN/alice ]; then
  ./parse-log.py $MAIN/alice.log $MAIN/alice
fi
if [ ! -d $MAIN/bob ]; then
  ./parse-log.py $MAIN/bob.log $MAIN/bob
fi
#
./draw-log.sh $MAIN/alice -x
./draw-log.sh $MAIN/bob -x


cat $MAIN/bob/import_transaction.csv | awk '{ s+=1; print $2"\t"s }' > $MAIN/imported_transaction-bob-summed.csv     
cat $MAIN/alice/propagate_transaction.csv | awk '{ s+=$3; print $2"\t"s }' > $MAIN/propagate_transaction-summed.csv    

# ./draw-propaged-imported-summed.sh propagated-imported $MAIN
./draw-validate-transaction.sh validate-transaction $MAIN

