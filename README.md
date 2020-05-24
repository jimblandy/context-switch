# Comparison of Rust async and Linux thread context switch time and memory use

These are a few programs that try to measure context switch time and task memory
use in various ways. In summary:

-   A context switch takes around 0.2µs between async tasks, versus 1.7µs
    between kernel threads. But this advantage goes away if the context switch
    is due to I/O readiness: both converge to 1.7µs. The async advantage also
    goes away in our benchmark if the program is pinned to a single core. So
    inter-core communication is something to watch out for.

-   Creating a new task takes ~300ns for an async task, versus ~17µs for a new
    kernel thread.

-   Memory consumption per task (i.e. for a task that doesn't do much) starts at
    around a few hundred bytes for an async task, versus around 20KiB (9.5KiB
    user, 10KiB kernel) for a kernel thread. This is a minimum: more demanding
    tasks will naturally use more.

-   It's no problem to create 250,000 async tasks, but I was only able to get my
    laptop to run 80,000 threads (4 core, two way HT, 32GiB).

These are probably not the limiting factors in your application, but it's nice
to know that the headroom is there.

## Measuring thread context time

The programs `thread-brigade` and `async-brigade` each create 500 tasks
connected by pipes (like a “bucket brigade”) and measure how long it takes to
propagate a single byte from the first to the last. One is implemented with
threads, and the other is implemented with the Tokio crate's async I/O.

    $ cd async-brigade/
    $ /bin/time cargo run --release
        Finished release [optimized] target(s) in 0.02s
         Running `/home/jimb/rust/context-switch/target/release/async-brigade`
    500 tasks, 10000 iterations:
    mean 1.795ms per iteration, stddev 82.016µs (3.589µs per task per iter)
    9.83user 8.33system 0:18.19elapsed 99%CPU (0avgtext+0avgdata 17144maxresident)k
    0inputs+0outputs (0major+2283minor)pagefaults 0swaps
    $

    $ cd ../thread-brigade
    $ /bin/time cargo run --release
        Finished release [optimized] target(s) in 0.02s
         Running `/home/jimb/rust/context-switch/target/release/thread-brigade`
    500 tasks, 10000 iterations:
    mean 2.657ms per iteration, stddev 231.822µs (5.313µs per task per iter)
    9.14user 27.88system 0:26.91elapsed 137%CPU (0avgtext+0avgdata 16784maxresident)k
    0inputs+0outputs (0major+3381minor)pagefaults 0swaps
    $

In these runs, I'm seeing 18.19s / 26.91s ≅ 0.68 or a 30% speedup from going
async. However, if I pin the threaded version to a single core, the speed
advantage of async disappears:

    $ taskset --cpu-list 1 /bin/time cargo run --release
        Finished release [optimized] target(s) in 0.02s
         Running `/home/jimb/rust/context-switch/target/release/thread-brigade`
    500 tasks, 10000 iterations:
    mean 1.709ms per iteration, stddev 102.926µs (3.417µs per task per iter)
    4.81user 12.50system 0:17.37elapsed 99%CPU (0avgtext+0avgdata 16744maxresident)k
    0inputs+0outputs (0major+3610minor)pagefaults 0swaps
    $

It would be interesting to see whether/how the number of tasks in the brigade
affects these numbers.

Per-thread resident memory use in `thread-brigade` is about 9.5KiB, whereas
per-async-task memory use in `async-brigade` is around 0.4KiB, a factor of ~20.
See 'Measuring memory use', below.

There are differences in the system calls performed by the two versions:

- In `thread-brigade`, each task does a single `recvfrom` and a `write` per
  iteration, taking 5.5µs.

- In `async-brigade`, each task does one `recvfrom` and one `write`, neither of
  which block, and then one more `recvfrom`, which returns `EAGAIN` and suspends
  the task. Then control returns to the executor. The reactor thread calls
  `epoll` to see which pipes are readable, and tells the executor which task to
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

-   `thread-creation` and `async-creation` attempt to measure the time
    required to create a thread / async task.

## Measuring memory use

The scripts `thread-brigade/rss-per-thread.sh` and
`async-brigade/rss-per-task.sh` run their respective brigade microbenchmarks
with varying numbers of tasks, and measure the virtual and resident memory
consumption at each count. You can then do a linear regression to see the memory
use of a single task. Note that `async-brigade/rss-per-task.sh` runs 10x as many
tasks, to keep the noise down.

As mentioned above, in my measurements, each thread costs around 9.5KiB, and
each async task costs around 0.4KiB, so the async version uses about 1/20th as
much memory as the threaded version.

To run this script, you'll need to have the Linux `pmap` utility installed; this
gives an accurate measurement of resident set size. On Fedora, this is included
in the `procps-ng` package.

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
    parent. (This is 33% of the kernel's default limit of around 32k, chosen by
    the systemd login manager.) To raise that limit, we can simply write another
    value to the file, as root:

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
