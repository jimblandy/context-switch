#!/usr/bin/env bash

set -eu

if ! [ -f Cargo.toml ]; then
    echo "Run in top-level directory of thread-brigade package." >&2
    exit 1
fi

cargo build --release

echo -e "num threads\tvirtual KiB\tresident KiB"
for ((n=1000; n <= 10000; n += 500)); do
    ../target/release/async-brigade --quiet --iters 10 --threads $n --command 'pmap -x {pid}' \
    | awk -v num_threads=$n '/^total/ { print num_threads "\t" $3 "\t" $4 }'
    # | awk -v num_threads=$n '
    #     /Active . Total Size/ { print num_threads "\t" $8 }
    # '
done
