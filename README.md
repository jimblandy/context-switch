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
  the task. Then control returns to the executor. The reactor thread calls
  `epoll` to see which pipes are readable, and tells the executor that task to
  run next. All this takes 3.6µs.

- In `one-thread-brigade`, we build the pipes but just have a single thread loop
  through them all and do the reads and writes. This gives us a baseline cost
  for the I/O operations themselves, which we can subtract off from the times in
  the other two programs, in hopes that the remainder reflects the cost of the
  context switches alone.

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

The other programs are minor variations, or make other measurements:

-   `async-mem-brigade` uses `tokio:sync::mpsc` channels to send `usize` values
    from one async channel to another. This performs the same number of
    task-to-task switches, but avoids the overhead of the pipe I/O. It seems
    that Tokio's channels do use futexes on Linux to signal readiness.

-   `one-thread-brigade` attempts to measure the cost of the pipe I/O alone, by
    creating all the pipes but having a single thread do all the reading and
    writing to propagate the byte from the first to the last.

-   `thread-creation` attempts to measure the time required to create a thread.

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
    *hierarchies*, where limits set on one cgroup apply to its descendant
    cgroups' members as well. The cgroups in a hierarchy have names that look
    like filesystem paths; the root cgroup is named `/`. The details get
    byzantine, but the upshot is that (in 2020, Linux 5.5) a typical Linux
    system has several independent hierarchies, each managing a different sort
    of resource: memory, cpu cycles, network bandwidth, and so on. For each
    hierarchy, every process/thread on the system belongs to exactly one cgroup,
    which it inherits from the process/thread that created it.

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
        cgroup.procs	       pids.current	      pids.max
        $

    The file `pids.max` shows the limit this cgroup imposes on my shell:

        $ cat /sys/fs/cgroup/pids/user.slice/user-1000.slice/user@1000.service/pids.max
        max
        $

    A limit of `max` means that there's no limit. However, for the `pids`
    controller, at least, limits set on parent cgroups also apply to their
    descendants, so we need to check our ancestor groups:

        $ cat /sys/fs/cgroup/pids/user.slice/user-1000.slice/pids.max
        10813
        $ cat /sys/fs/cgroup/pids/user.slice/pids.max
        max
        $ cat /sys/fs/cgroup/pids/pids.max
        cat: /sys/fs/cgroup/pids/pids.max: No such file or directory
        $

    Apparently there's a limit of 10813 pids imposed by my shell's cgroup's
    parent. (This is 33% of the To raise that limit, we can simply write another value to the file,
    as root:

        $ sudo sh -c 'echo 100000 > /sys/fs/cgroup/pids/user.slice/user-1000.slice/pids.max'

-   The kernel parameter `kernel.threads-max` is a system-wide limit on the
    number of threads. You probably won't run into this.

        $ sysctl kernel.threads-max
        kernel.threads-max = 255208
        $

-   There is a limit on the number of processes that can run under a given real
    user ID:

        $ ulimit -u
        127604
        $

    At the system call level, this is the `getrlimit(2)` system call's
    `RLIMIT_NPROC` resource. This, too, you're unlikely to run into.

-   The default thread stack size is 8MiB:

        $ ulimit -s
        8192
        $

    You might expect this to limit a 32GiB (x86_64) machine to 4096 threads, but
    the kernel only allocates physical memory to a stack as the thread touches
    its pages, so the actual initial memory consumption of a thread in user
    space is actually only around 8kiB. At this size, 32GiB could accommodate
    4Mi threads. Again, this is unlikely to be the limiting factor.

    Although it doesn't matter, `thread-brigade` program in this repository
    requests a 1MiB stack for each thread, which is plenty for our purposes.

With these changes made, I was able to run `thread-brigade` with 80000 tasks.
