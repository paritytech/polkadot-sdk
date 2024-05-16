#!/bin/bash -x

usage() {
  echo "usage $0 data-file graph-file [options]"
  echo " -x do not run eog"
  exit -1
}

WDIR=$1


GRAPHFILE=$WDIR.png
if [ ! -f $WDIR/../start ]; then
  echo "$WDIR/../start time stamp does not exists"
  exit -1
fi

if [ ! -f $WDIR/../end ]; then
  echo "$WDIR/../end time stamp does not exists"
  exit -1
fi

START_TIME=`cat $WDIR/../start`
END_TIME=`cat $WDIR/../end`


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

# if [ -z $DATAFILE ]; then
#   usage
# fi

echo $GRAPHFILE $RUNEOG

# if [ ! -f $DATAFILE ]; then
#   echo "$DATAFILE does not exists..."
#   exit -1
# fi

cat > $GRAPHFILE.gnu << end-of-gnuplot
set terminal pngcairo  enhanced font "arial,10" fontscale 3.0 size 6560, 3500
set output '$GRAPHFILE'
set lmargin at screen 0.025
set rmargin at screen 0.975

set xdata time
set timefmt "%H:%M:%S"
# set xrange ["14:10:54":"14:10:57.500"]
set xrange ["$START_TIME":"$END_TIME"]
set format x "%H:%M:%2.2S"
set xtics 1
set mxtics 10

set y2tics nomirror
set my2tics 10

set ytics nomirror
set grid ytics mytics
set grid xtics mxtics
set grid y2tics my2tics

set key noenhanced

set multiplot

plot_height = 0.138
margin = 0.005
height = plot_height + margin

y_position = 1.0 - plot_height
set size 1.0,plot_height

set tmargin 2


file_line_count(f) = system(sprintf("wc -l < '%s'", f))

########################################
file="$WDIR/import.csv"
set origin 0.0,y_position
y_position = y_position - height
set style line 1 lc rgb 'red' lt 1 lw 1 pt 1 pi -1 ps 3
set y2range [0:3]
filter(x)=(x==2)?(2):(1/0)
plot \
  file using "time":"block_number" with steps ls 1 axes x1y1 title "import", \
  '' u "time":"block_number" with points pt 2 ps 3 lc rgb "blue" title "NBB", \
  "$WDIR/txpool_maintain.csv" using "time":(filter(column("event"))) with points ps 3 pt 7 lc rgb "green" axes x1y2 title "Finalized"
unset y2range

########################################
set origin 0.0,y_position
y_position = y_position - height
plot \
  "$WDIR/block_proposing.csv" using "time":"extrinsics_count" with points pt 5 ps 3.0 lc rgb 'dark-green' axes x1y1 title "block proposing (tx count)", \
  "$WDIR/block_proposing_start.csv" using "time":"value" with points pt 5 ps 2.0 lc rgb 'red' axes x1y1 title "block proposing start"

########################################
file="$WDIR/validate_transaction.csv"
set origin 0.0,y_position
y_position = y_position - height
set yrange [0:1000]
plot \
  file using "time":"duration" with points pt 2 lc rgb "blue" axes x1y1 title "validate_transaction"
unset yrange

########################################
file="$WDIR/import_transaction.csv"
if (file_line_count(file) + 0 > 1) {
  set origin 0.0,y_position
  y_position = y_position - height
  plot \
    file using "time":"duration" with points pt 2 lc rgb "dark-turquoise" axes x1y1 title "import transaction"
}

########################################
file="$WDIR/propagate_transaction.csv"
if (file_line_count(file) + 0 > 1) {
  set origin 0.0,y_position
  y_position = y_position - height
  plot \
    file using "time":"value" with points pt 2 lc rgb "dark-turquoise" axes x1y1 title "propagate transaction"
}

########################################
file="$WDIR/txpool_maintain.csv"
set origin 0.0,y_position
y_position = y_position - height
set style line 1 lc rgb 'red' lt 1 lw 1 pt 1 pi -1 ps 0.7
set style line 2 lc rgb 'blue' lt 1 lw 1 pt 1 pi -1 ps 0.7
set style line 3 lc rgb 'black' lt 2 lw 1 pt 1 pi -1 ps 0.7
set y2range [*<0:3<*]
plot \
  file using "time":"unwatched_txs" with steps ls 1 axes x1y1 title "unwatched txs", \
  file using "time":"watched_txs" with steps ls 2 axes x1y1 title "watched txs", \
  file using "time":"views_count" with steps ls 3 axes x1y2 title "views count", \

########################################
file="$WDIR/txpool_maintain.csv"
set origin 0.0,y_position
y_position = y_position - height
set style line 1 lc rgb 'red' lt 1 lw 1 pt 1 pi -1 ps 0.7
set style line 1 lc rgb 'blue' lt 1 lw 1 pt 1 pi -1 ps 0.7
plot \
  file using "time":"duration" with points pt 7 ps 3.0 lc rgb "blue" axes x1y1 title "maintain duration"

################################################################################

unset multiplot

end-of-gnuplot


gnuplot -c $GRAPHFILE.gnu
echo "gnuplot done..."
if [ $RUNEOG == 1 ]; then
  echo "--------------------------------------------------------------------------------"
  ls -al $GRAPHFILE
  eog  $GRAPHFILE
fi

