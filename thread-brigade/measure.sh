#!/usr/bin/env bash

set -eu

if ! [ -f Cargo.toml ]; then
    echo "Run in top-level directory of thread-brigade package." >&2
    exit 1
fi

cargo build --release

echo -e "num threads\tvirtual KiB\tresident KiB"
for ((n=100; n <= 1000; n += 50)); do
    ../target/release/thread-brigade --quiet --iters 1000 --threads $n --command 'pmap -x {pid}' \
    | awk -v num_threads=$n '/^total/ { print num_threads "\t" $3 "\t" $4 }'
    # | awk -v num_threads=$n '
    #     /Active . Total Size/ { print num_threads "\t" $8 }
    # '
done
