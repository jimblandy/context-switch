# Comparison of Rust async and Linux thread context switch time

These are two programs that create 500 tasks connected by pipes and measure how
long it takes to propagate a single byte from the first to the last. One is
implemented with threads, and the other is implemented with the Tokio crate's
async I/O.
