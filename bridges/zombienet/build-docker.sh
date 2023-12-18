#!/bin/bash
set -eux
CONTEXT=`dirname "$0"`
docker build -f $CONTEXT/Dockerfile $CONTEXT -t bzn
docker run -it -p 9942:9942 -p 9910:9910 -p 8943:8943 -p 9945:9945 -p 9010:9010 -p 8945:8945 bzn
