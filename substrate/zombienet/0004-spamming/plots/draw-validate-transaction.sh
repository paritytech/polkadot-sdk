#!/bin/bash

usage() {
  echo "usage $0 data-file graph-file [options]"
  echo " -x do not run eog"
  exit -1
}

DATAFILE=$1
DIR=$2
GRAPHFILE=$DIR/$DATAFILE.png

shift 1

RUNEOG=1

while getopts "x" o; do
    case "${o}" in
        x)
            RUNEOG=0
            ;;
        *)
            usage
            ;;
    esac
done

echo $DATAFILE $GRAPHFILE $RUNEOG

if [ ! -d $DIR ]; then
  echo "$DIR does not exists..."
  exit -1
fi

cat > $GRAPHFILE.gnu << end-of-gnuplot
set terminal pngcairo  enhanced font "arial,10" fontscale 1.0 size 4560, 2560
set output '$GRAPHFILE'
set datafile separator "\t"

set multiplot
set lmargin at screen 0.025
set rmargin at screen 0.975

plot_height = 0.45
margin = 0.005
height = plot_height + margin

y_position = 1.0 - plot_height
set size 1.0,plot_height

set tmargin 2

set title noenhanced


set ytics nomirror
set grid ytics mytics
set grid xtics mxtics

set xdata time
set timefmt "%H:%M:%S"
# set xrange ["14:10:54":"14:10:57.500"]
# set xrange ["15:54:55":"15:55:05"]
set format x "%H:%M:%2.2S"
# set xtics 6
# set mxtics 6

set key autotitle columnhead

set y2tics nomirror
set my2tics 10
set grid y2tics my2tics
set format y2 "%20.10f"

set origin 0.0,y_position
y_position = y_position - height

set yrange [0:1000]

set title "alice"
plot \
   "$DIR/alice/validate_transaction.csv" using 2:4 with points ps 2 axes x1y1 title "validate transaction"

set origin 0.0,y_position
y_position = y_position - height

set title "bob:
plot \
   "$DIR/bob/validate_transaction.csv" using 2:4 with points ps 2 axes x1y1 title "validate transaction"

end-of-gnuplot


gnuplot -c $GRAPHFILE.gnu
echo "gnuplot done..."
if [ $RUNEOG == 1 ]; then
  eog  $GRAPHFILE
fi

