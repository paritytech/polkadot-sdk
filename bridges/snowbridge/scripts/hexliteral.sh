#!/bin/bash
# Creates a string constant from STDIN
echo "const DATA: &'static str = concat!("
cat - | fold | sed 's/^.*/\t"&",/'
echo ");"