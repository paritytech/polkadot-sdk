#!/usr/bin/env python3

import argparse
import os
import sys
import subprocess
import re
from datetime import datetime

def usage():
    print("usage: script.py data-file [options]")
    print(" -x do not run eog")
    exit(-1)


def sanitize_string(string):
    return re.sub(r'[^A-Za-z0-9_]+', '_', string)

def file_line_count(file):
    with open(file, 'r') as f:
        return sum(1 for _ in f)

def import_graph():
    return f"""
set style line 1 lc rgb 'red' lt 1 lw 1 pt 1 pi -1 ps 3
filter(v,x)=(x==v)?(v):(1/0)
set y2range [0:10]
plot \
  file1 using (combine_datetime("date","time")):"block_number" with steps ls 1 axes x1y1 title "import", \
  file1 using (combine_datetime("date","time")):"block_number" with points pt 2 ps 3 lc rgb "blue" title "new block", \
  file2 using (combine_datetime("date","time")):(filter(2,column("event"))) with points lc rgb "red" pt 1 ps 3 title "Finalized" axes x1y2, \
  file2 using (combine_datetime("date","time")):(filter(1,column("event"))) with points lc rgb "blue" pt 2 ps 3 title "NewBestBlock" axes x1y2, \
  file2 using (combine_datetime("date","time")):(filter(0,column("event"))) with points lc rgb "green" pt 3 ps 3 title "NewBlock" axes x1y2 
unset y2range
"""

def import_transaction_graph():
    return f"""
plot \\
  file1 using (combine_datetime("date","time")):"duration" with points pt 2 lc rgb "dark-turquoise" axes x1y1 title "import transaction"
"""

def propagate_transaction_graph():
    return f"""
propagate_transaction_cumulative_sum = 0
propagate_transaction_running_sum(column) = (propagate_transaction_cumulative_sum = propagate_transaction_cumulative_sum + column, propagate_transaction_cumulative_sum)
plot \\
  file1 using (combine_datetime("date","time")):(propagate_transaction_running_sum(column("value"))) with points pt 2 lc rgb "dark-turquoise" axes x1y1 title "propagate transaction"
"""

def propagate_transaction_failure_graph():
    return f"""

propagate_transaction_failure_cumulative_sum = 0
propagate_transaction_failure_running_sum(column) = (propagate_transaction_failure_cumulative_sum = propagate_transaction_failure_cumulative_sum + column, propagate_transaction_failure_cumulative_sum)
plot \\
  file1 using (combine_datetime("date","time")):(propagate_transaction_failure_running_sum(column("value"))) with points pt 5 ps 1.0 lc rgb 'dark-green' axes x1y1 title "propagate transaction failure"
"""

def txpool_maintain_graph():
    return f"""
set style line 1 lc rgb 'red' lt 1 lw 1 pt 1 pi -1 ps 0.7
set style line 2 lc rgb 'blue' lt 1 lw 1 pt 1 pi -1 ps 0.7
set style line 3 lc rgb 'black' lt 2 lw 2 pt 1 pi -1 ps 0.7

set y2tics nomirror
set my2tics 10

plot \\
  file1 using (combine_datetime("date","time")):"unwatched_txs" with steps ls 1 axes x1y1 title "unwatched txs", \\
  file1 using (combine_datetime("date","time")):"watched_txs" with steps ls 2 axes x1y1 title "watched txs", \\
  file1 using (combine_datetime("date","time")):"views_count" with steps ls 3 axes x1y2 title "views count"

unset y2tics
unset my2tics
"""

def txpool_maintain_duration_graph():
    return f"""
set logscale y 10
set style line 1 lc rgb 'red' lt 1 lw 1 pt 1 pi -1 ps 0.7
set style line 1 lc rgb 'blue' lt 1 lw 1 pt 1 pi -1 ps 0.7
plot \\
  file1 using (combine_datetime("date","time")):"duration" with points pt 7 ps 3.0 lc rgb "blue" axes x1y1 title "maintain duration"
unset logscale
"""

def txpool_maintain_duration_histogram():
    return f"""
reset
binwidth=1000;
bin(x,width)=width*floor(x/width) + binwidth/2.0;
skip_first_bin(x) = (x >= binwidth) ? x : NaN
plot file1 using (bin(skip_first_bin(column("duration")),binwidth)):(1.0) smooth freq with boxes lc rgb "blue" fs solid 0.5;
"""

def validate_transaction():
    return f"""
set logscale y 2
plot \
  file1 using (combine_datetime("date","time")):"duration" with points pt 2 lc rgb "blue" axes x1y1 title "validate_transaction"
unset logscale
"""

def validate_transaction_count():
    return f"""
validate_tx_count = 0
validate_tx_running_sum(column) = (validate_tx_count = validate_tx_count + 1, validate_tx_count)
plot \\
  file1 using (combine_datetime("date","time")):(validate_tx_running_sum(column("duration"))) with points pt 2 lc rgb "blue" axes x1y1 title "validate_transaction count"
"""



def block_proposing():
    return f"""
plot \\
  file1 using (combine_datetime("date","time")):"extrinsics_count" with points pt 5 ps 3.0 lc rgb 'dark-green' axes x1y1 title "block proposing (tx count)", \\
  file2 using (combine_datetime("date","time")):"value" with points pt 5 ps 2.0 lc rgb 'red' axes x1y1 title "block proposing start"
"""

def submit_one():
    return f"""
submit_one_cumulative_sum = 0
submit_one_running_sum(column) = (submit_one_cumulative_sum = submit_one_cumulative_sum + column, submit_one_cumulative_sum)
plot \\
  file1 using (combine_datetime("date","time")):(submit_one_running_sum(column("value"))) with points pt 5 ps 1.0 lc rgb 'dark-green' axes x1y1 title "submit_one"
"""

def tmp_graph():
    return f"""
tmp_graph_cumulative_sum = 0
plot \\
  file1 using (combine_datetime("date","time")):(tmp_graph_running_sum(column("value"))) with points pt 5 ps 1.0 lc rgb 'dark-green' axes x1y1 title sprintf("%s", file1)
"""



GRAPH_FUNCTIONS = {
    "import": {
        "file_names": ["import.csv", "txpool_maintain.csv"],
        "function_name": import_graph
        },
    "import_transaction": {
        "file_names": ["import_transaction.csv"],
        "function_name": import_transaction_graph
        },
    "propagate_transaction": {
        "file_names": ["propagate_transaction.csv"],
        "function_name": propagate_transaction_graph
        },
    "propagate_transaction_failure": {
        "file_names": ["propagate_transaction_failure.csv"],
        "function_name": propagate_transaction_failure_graph
        },
    "txpool_maintain": {
        "file_names": ["txpool_maintain.csv"],
        "function_name": txpool_maintain_graph
        },
    "txpool_maintain_duration": {
        "file_names": ["txpool_maintain.csv"],
        "function_name": txpool_maintain_duration_graph
        },
    "txpool_maintain_hist": {
        "file_names": ["txpool_maintain.csv"],
        "function_name": txpool_maintain_duration_histogram
        },
    "validate_transaction_count": {
        "file_names": ["validate_transaction.csv"],
        "function_name": validate_transaction_count
        },
    "validate_transaction": {
        "file_names": ["validate_transaction.csv"],
        "function_name": validate_transaction
        },
    "block_proposing": {
        "file_names": ["block_proposing.csv", "block_proposing_start.csv"],
        "function_name": block_proposing
        },
    "submit_one": {
        "file_names": ["submit_one.csv"],
        "function_name": submit_one
        }
}

def main():
    parser = argparse.ArgumentParser(description='Generate graphs showing datapoints from some predefined csv files using gnuplot.')
    parser.add_argument('data_directory', help='Path to the data directory')
    parser.add_argument('-x', action='store_true', help='Do not run eog')
    parser.add_argument('-r', action='append', dest='tmp_graphs', help='tmp graphs to be added')
    supported_graphs = ', '.join(GRAPH_FUNCTIONS.keys())
    parser.add_argument('--graphs', help=f"Comma-separated list of graphs to include: {supported_graphs}")

    args = parser.parse_args()

    wdir = args.data_directory
    graphfile = f"{wdir}.png"

    if not os.path.isfile(os.path.join(wdir, '../start')):
        print(f"{os.path.join(wdir, '../start')} time stamp does not exist")
        exit(-1)

    if not os.path.isfile(os.path.join(wdir, '../end')):
        print(f"{os.path.join(wdir, '../end')} time stamp does not exist")
        exit(-1)

    with open(os.path.join(wdir, '../start'), 'r') as f:
        start_time = f.read().strip()

    with open(os.path.join(wdir, '../end'), 'r') as f:
        end_time = f.read().strip()

    format = "%Y-%m-%d %H:%M:%S"
    duration_in_seconds = (datetime.strptime(end_time, format) - datetime.strptime(start_time, format)).total_seconds()

    runeog = not args.x


    graphs = [];
    for tmp_g in args.tmp_graphs:
        t = sanitize_string(tmp_g)
        print("t -> ",t);
        GRAPH_FUNCTIONS[t] = { "file_names": [f"{t}.csv"], "function_name": tmp_graph }
        graphs.append(t)

    selected_graphs = graphs

    if not args.graphs is None:
        selected_graphs.extend(args.graphs.split(','))

    missing_graphs = [graph for graph in selected_graphs if graph not in GRAPH_FUNCTIONS]
    if missing_graphs:
        print(f"invalid graphs given: {missing_graphs}, supported graphs are: {supported_graphs}")
        sys.exit()

    num_graphs = len(selected_graphs)
    plot_height = 1.0 / num_graphs - 0.005

    gnuplot_content = f"""
set terminal pngcairo  enhanced font "arial,10" fontscale 3.0 size 6560, 3500
set output '{graphfile}'
set lmargin at screen 0.025
set rmargin at screen 0.975

set xdata time
set timefmt "%Y-%m-%d %H:%M:%S"
set xrange ["{start_time}":"{end_time}"]
set timefmt "%Y-%m-%dT%H:%M:%S"
set format x "%H:%M:%2.2S"
set xtics {duration_in_seconds} / 10
set mxtics 10
set grid xtics mxtics

set ytics nomirror
set grid ytics mytics

set key noenhanced

set multiplot

plot_height = {plot_height}
margin = 0.005
height = plot_height + margin

y_position = 1.0 - plot_height
set size 1.0,plot_height

set tmargin 2
file_line_count(f) = system(sprintf("wc -l < '%s'", f))
combine_datetime(date_col,time_col) = strcol(date_col) . "T" . strcol(time_col)

tmp_graph_cumulative_sum = 0
tmp_graph_running_sum(column) = (tmp_graph_cumulative_sum = tmp_graph_cumulative_sum + column, tmp_graph_cumulative_sum)
"""

    for graph in selected_graphs:
        if graph in GRAPH_FUNCTIONS:
            data_files = GRAPH_FUNCTIONS[graph]["file_names"]
            data_file_index = 1
            add_graph = True
            files_content = ""

            for data_file in data_files:
                full_data_file_path = os.path.join(wdir, f"{data_file}")
                files_content += f"""
file{data_file_index}="{os.path.join(wdir, f"{data_file}")}"
"""
                data_file_index+=1
                if file_line_count(full_data_file_path) <= 1:
                    add_graph = False
                    print(f"{full_data_file_path} is empty")
                    break

            if add_graph:
                gnuplot_content += f"""
set origin 0.0,y_position
y_position = y_position - height
{files_content}
"""
                gnuplot_content += GRAPH_FUNCTIONS[graph]["function_name"]()

    gnuplot_content += """
################################################################################

unset multiplot
"""

    with open(f"{graphfile}.gnu", 'w') as f:
        f.write(gnuplot_content)

    subprocess.run(['gnuplot', f"{graphfile}.gnu"])
    print("gnuplot done...")

    if runeog:
        print("--------------------------------------------------------------------------------")
        subprocess.run(['ls', '-al', graphfile])
        subprocess.run(['eog', graphfile])

if __name__ == "__main__":
    main()
