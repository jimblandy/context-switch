# Comparison of Rust async and Linux thread context switch time

These are two programs that create 500 tasks connected by pipes and measure how
long it takes to propagate a single byte from the first to the last. One is
implemented with threads, and the other is implemented with the Tokio crate's
async I/O.

## Running tests with large numbers of threads

It's interesting to play with the number of tasks to see how that affects the
relative speed of the async and threaded bucket brigades. But in order to test
large numbers of threads, you may need to lift some system-imposed limits.

On Linux:

-   You will run out of file descriptors. Each worker needs two file descriptors,
    one for the reading end of the upstream pipe, and one for the writing end of
    the downstream pipe. The process also needs a few file descriptors for
    miscellaneous purposes. For 50000 workers, say:

        $ ulimit -n 100010

-   You will run out of process id numbers. Each thread needs its own pid. For
    50000 workers, say:

        $ sudo sysctl kernel.pid_max=1000000

    This is overkill, but why worry about this?

-   You will run out of memory map areas. Each thread has its own stack, with an
    unmapped guard page at the low end to catch stack overflows. There seem to
    be other constraints as well. In practice, this seems to work for 50000
    workers:

        $ sudo sysctl vm.max_map_count=200000

With these changes made, I was able to run thread-brigade with 50000 workers.
