# Comparison of Rust async and Linux thread context switch time

These are a few programs that try to measure context switch time in various ways.

The programs `thread-brigade` and `async-brigade` each create 500 tasks
connected by pipes (like a “bucket brigade”) and measure how long it takes to
propagate a single byte from the first to the last. One is implemented with
threads, and the other is implemented with the Tokio crate's async I/O.

    $ cd async-brigade
    $ time cargo run --release
       Compiling async-brigade v0.1.0 (/home/jimb/rust/async/context-switch/async-brigade)
        Finished release [optimized] target(s) in 0.56s
         Running `/home/jimb/rust/async/context-switch/target/release/async-brigade`
    10000 iterations, 500 tasks, mean 1.858ms per iteration, stddev 129.544µs
    12.49user 8.39system 0:19.37elapsed 107%CPU (0avgtext+0avgdata 152832maxresident)k
    $

    $ cd ../thread-brigade
    $ time cargo run --release
       Compiling thread-brigade v0.1.0 (/home/jimb/rust/async/context-switch/thread-brigade)
        Finished release [optimized] target(s) in 0.26s
         Running `/home/jimb/rust/async/context-switch/target/release/thread-brigade`
    10000 iterations, 500 tasks, mean 2.763ms per iteration, stddev 537.584µs
    10.16user 27.47system 0:28.26elapsed 133%CPU (0avgtext+0avgdata 133520maxresident)k
    0inputs+5568outputs (13major+23830minor)pagefaults 0swaps
    $

In these runs, I'm seeing 1.8 / 2.7 ≅ 0.67 or a 30% speedup from going async.
The `thread-brigade` version has a resident set size of about 6.1MiB, whereas
`async-brigade` runs in about 2.2MiB.

There are differences in the system calls performed by the two versions:

- In `thread-brigade`, each task does a single `recvfrom` and a `write` per
  iteration, taking 5.5µs.

- In `async-brigade`, each task does one `recvfrom` and one `write`, neither of
  which block, and then one more `recvfrom`, which returns `EAGAIN` and suspends
  the task. Then control returns to the executor, which calls `epoll` to see
  which task to wake up next. All this takes 3.6µs.

The `async-brigade` performance isn't affected much if we switch from Tokio's
default multi-thread executor to a single-threaded executor, so it's not
spending much time in kernel context switches. `thread-brigade` does a kernel
context switch from each task to the next. I think this means that context
switches are more expensive than a `recvfrom` and `epoll` system call.

If we run the test with 50000 tasks (and reduce the number of iterations to
100), the speedup doesn't change much, but `thread-brigade` requires a 466MiB
resident set, whereas `async-brigade` runs in around 21MiB. That's 10kiB of
memory being actively touched by each task, versus 0.4kiB, about a twentieth.
This isn't just the effect of pessimistically-sized thread stacks: we're looking
at the resident set size, which shouldn't include pages allocated to the stack
that the thread never actually touches. So the way Rust right-sizes futures
seems really effective.

This microbenchmark doesn't do much, but a real application would add to each
task's working set, and that difference might become less significant. But I was
able to run async-brigade with 250,000 tasks; I wasn't able to get my laptop
to run 250,000 threads at all.

The other programs are minor variations:

-   `async-mem-brigade` uses `tokio:sync::mpsc` channels to send `usize` values
    from one async channel to another. This performs the same number of
    task-to-task switches, but avoids the overhead of the pipe I/O. It seems
    that Tokio's channels do use futexes on Linux to signal readiness.

-   `one-thread-brigade` attempts to measure the cost of the pipe I/O alone, by
    creating all the pipes but having a single thread do all the reading and
    writing to propagate the byte from the first to the last.

## Running tests with large numbers of threads

It's interesting to play with the number of tasks to see how that affects the
relative speed of the async and threaded bucket brigades. But in order to test
large numbers of threads, you may need to remove some of your system's
guardrails.

On Linux:

-   You will run out of file descriptors. Each task needs two file descriptors,
    one for the reading end of the upstream pipe, and one for the writing end of
    the downstream pipe. The process also needs a few file descriptors for
    miscellaneous purposes. For 50000 tasks, say:

        $ ulimit -n 100010

-   You will run out of process id numbers. Each thread needs its own pid. For
    50000 tasks, say:

        $ sudo sysctl kernel.pid_max=1000000

    This is overkill, but why worry about this?

-   You will run out of memory map areas. Each thread has its own stack, with an
    unmapped guard page at the low end to catch stack overflows. There seem to
    be other constraints as well. In practice, this seems to work for 50000
    tasks:

        $ sudo sysctl vm.max_map_count=200000

-   Process ID numbers can also be limited by the `pids` cgroup controller.

    A cgroup is a collection of processes/threads on which you can impose system
    resource limits as a group. Cgroups themselves are arranged in trees called
    *hierarchies*, with names that look like filesystem paths, and a 'root'
    cgroup named `/`. There can be separate cgroup hierarchies for different
    sorts of resources: memory, cpu cycles, network bandwidth, and so on. Within
    a hierarchy, every process/thread on the system belongs to exactly one
    cgroup, which it inherits from its parent process/thread.

    The `pids` controller limits the number of process IDs a cgroup can have.
    You can see which `pids` cgroup your shell is in like this:

        $ grep pids /proc/$$/cgroup
        2:pids:/user.slice/user-1000.slice/user@1000.service

    This indicates that, in the `pids` hierarchy, my shell is in a cgroup named
    `/user.slice/user-1000.slice/user@1000.service`.

    On Fedora, at least, the `pids` controller's hierarchy is reflected in the
    ordinary filesystem as a directory tree under `/sys/fs/cgroup/pids`, so my
    shell's cgroup is here:

        $ ls /sys/fs/cgroup/pids/user.slice/user-1000.slice/user@1000.service
        cgroup.clone_children  notify_on_release  pids.events  tasks
        cgroup.procs	       pids.current	  pids.max
        $

    The file `pids.max` shows the limit this cgroup imposes on my shell:

        $ cat /sys/fs/cgroup/pids/user.slice/user-1000.slice/user@1000.service/pids.max
        max
        $

    A limit of `max` means that there's no limit. However, for the `pids`
    controller, limits set on parent cgroups also apply to their descendants,
    so we need to check our ancestor groups:

        $ cat /sys/fs/cgroup/pids/user.slice/user-1000.slice/pids.max
        10813
        $ cat /sys/fs/cgroup/pids/user.slice/pids.max
        max
        $ cat /sys/fs/cgroup/pids/pids.max
        cat: /sys/fs/cgroup/pids/pids.max: No such file or directory
        $

    Apparently there's a limit of 10813 pids imposed by my shell's cgroup's
    parent. To raise that limit, we can simply write another value to the file,
    as root:

        $ sudo sh -c 'echo 100000 > /sys/fs/cgroup/pids/user.slice/user-1000.slice/pids.max'

With these changes made, I was able to run `thread-brigade` with 49000 tasks.
