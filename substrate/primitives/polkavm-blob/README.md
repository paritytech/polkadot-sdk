Read and modify custom sections of PolkaVM program blobs.

PolkaVM program blobs can carry optional custom sections that are skipped by every
parser. This crate reads such sections by id and appends or replaces them while
keeping the blob's length metadata consistent.

License: Apache-2.0