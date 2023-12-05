#!/bin/bash
set -xeu

# This will allow us to run whichever binary the user wanted
# with arguments passed through `docker run`
# e.g `docker run -it substrate-relay --dev --tmp`
/home/user/$PROJECT $@
