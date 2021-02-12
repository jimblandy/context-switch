# Comparison of Rust async and Linux thread context switch time and memory use

These are a few programs that try to measure context switch time and task memory
use in various ways. In summary:

-   A context switch takes around 0.2µs between async tasks, versus 1.7µs
    between kernel threads. But this advantage goes away if the context switch
    is due to I/O readiness: both converge to 1.7µs. The async advantage also
    goes away in our microbenchmark if the program is pinned to a single core.
    So inter-core communication is something to watch out for.

-   Creating a new task takes ~0.3µs for an async task, versus ~17µs for a new
    kernel thread.

-   Memory consumption per task (i.e. for a task that doesn't do much) starts at
    around a few hundred bytes for an async task, versus around 20KiB (9.5KiB
    user, 10KiB kernel) for a kernel thread. This is a minimum: more demanding
    tasks will naturally use more.

-   It's no problem to create 250,000 async tasks, but I was only able to get my
    laptop to run 80,000 threads (4 core, two way HT, 32GiB).

These are probably not the limiting factors in your application, but it's nice
to know that the headroom is there.

## Measuring thread context switch time

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

I don't know why.

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
in the `procps-ng` package. (Pull requests for info about other major
distributions welcome.)

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

-   You may run out of process id numbers. Each thread needs its own pid. So,
    perhaps something like:

        $ sudo sysctl kernel.pid_max=4194304

    This is overkill, but why worry about this? (The number above is the default
    in Fedora 33, 4 × 1024 × 1024; apparently systemd was worried about pid
    rollover.)

-   You will run out of memory map areas. Each thread has its own stack, with an
    unmapped guard page at the low end to catch stack overflows. There seem to
    be other constraints as well. In practice, this seems to work for 50000
    tasks:

        $ sudo sysctl vm.max_map_count=200000

-   Process ID numbers can also be limited by the `pids` cgroup controller.

    A cgroup is a collection of processes on which you can impose system
    resource limits as a group. Every process belongs to exactly one cgroup.
    When one process creates another, the new process is placed in the same
    cgroup as its parent.

    Cgroups are arranged in a tree, where limits set on a cgroup apply to that
    group and all its descendants. Only leaf cgroups actually contain
    processes/threads. The cgroups in the hierarchy have names that look like
    filesystem paths; the root cgroup is named `/`.

    You can see which cgroup your shell belongs to like this:

        $ cat /proc/$$/cgroup
        0::/user.slice/user-1000.slice/gargle/howl.scope

    This indicates that my shell is in a cgroup named
    `/user.slice/user-1000.slice/gargle/howl.scope`. The names can get quite
    long, so this example is simplified.

    On Fedora, at least, the cgroup hierarchy is reflected in the ordinary
    filesystem as a directory tree under `/sys/fs/cgroup`, so my shell's
    cgroup appears as a directory here:

        $ ls /sys/fs/cgroup/user.slice/user-1000.slice/gargle/howl.scope
        cgroup.controllers	    cpu.stat	         memory.pressure
        cgroup.events		    io.pressure	         memory.stat
        cgroup.freeze		    memory.current	     memory.swap.current
        cgroup.max.depth	    memory.events	     memory.swap.events
        cgroup.max.descendants	memory.events.local  memory.swap.high
        cgroup.procs		    memory.high	         memory.swap.max
        cgroup.stat		        memory.low	         pids.current
        cgroup.subtree_control	memory.max	         pids.events
        cgroup.threads		    memory.min	         pids.max
        cgroup.type		        memory.numa_stat
        cpu.pressure		    memory.oom.group
        $

    You can inspect and manipulate cgroups by looking at these files. Some
    represent different resources that can be limited, while others relate to
    the cgroup hierarchy itself.

    In particular, the file `pids.max` shows the limit this cgroup imposes on my
    shell:

        $ cat /sys/fs/cgroup/user.slice/user-1000.slice/gargle/howl.scope/pids.max
        max
        $

    A limit of `max` means that there's no limit. But limits set on parent
    cgroups also apply to their descendants, so we need to check our ancestor
    groups:

        $ cat /sys/fs/cgroup/user.slice/user-1000.slice/gargle/pids.max
        10813
        $ cat /sys/fs/cgroup/user.slice/user-1000.slice/pids.max
        84184
        $ cat /sys/fs/cgroup/user.slice/pids.max
        max
        $ cat /sys/fs/cgroup/pids.max
        cat: /sys/fs/cgroup/pids.max: No such file or directory
        $

    Apparently there's a limit of 10813 pids imposed by my shell's cgroup's
    parent, and a higher limit of 84184 pids set for me as a user. (On Fedora,
    these limits are established by systemd configuration files.) To raise that
    limit, we can simply write another value to these files, as root:

        $ sudo sh -c 'echo 100000 > /sys/fs/cgroup/user.slice/user-1000.slice/pids.max'
        $ sudo sh -c 'echo max    > /sys/fs/cgroup/user.slice/user-1000.slice/gargle/pids.max'

    The cgroup machinery seems to vary not only from one Linux distribution to
    the next, but even from one version to another. So while I hope this is
    helpful, you may need to consult other documentation. `man cgroups(7)` is a
    good place to start, but beware, it makes my explanation here look short.

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
    its pages, so the initial memory consumption of a thread in user space is
    actually only around 8kiB. At this size, 32GiB could accommodate 4Mi
    threads. Again, this is unlikely to be the limiting factor.

    Although it doesn't matter, `thread-brigade` program in this repository
    requests a 1MiB stack for each thread, which is plenty for our purposes.

With these changes made, I was able to run `thread-brigade` with 80000 tasks.

## Does any of this matter?

In GitHub issue #1, @spacejam raised a good point:

> overall, there are a lot of things here that really fade into insignificance
> when you consider the simple effort required to deserialize JSON or handle
> TLS. People often see that there's some theoretical benefit of async and then
> they accept far less ergonomic coding styles and the additional bug classes
> that only happen on async due to accidental blocking etc... despite the fact
> that when you consider a real-world deployed application, those "benefits"
> become indistinguishable from noise. However, due to the additional bug
> classes and worse ergonomics, there is now less energy for actually optimizing
> the business logic, which is where all of the cycles and resource use are
> anyway, so in-practice async implementations tend to be buggier and slower.

Below is my reply to them, lightly edited:

> I have a few responses to this.
>
> First of all, the reason I carried out the experiments in this repo in the
> first place was that I basically agreed with all of your points here. I think
> async is wildly oversold as "faster" without any real investigation into why
> that would be. It is hard to pin down exactly how the alleged advantages would
> arise. The same I/O operations have to be carried out either way (or worse);
> kernel context switches have been heavily optimized over the years (although
> the Spectre mitigations made them worse); and the whole story of the creation
> of NPTL was about it beating IBM's competing M-on-N thread implementation
> (which I see as analogous to async task systems) in the very microbenchmarks
> in which the M-on-N thread library was expected to have an advantage.
>
> However, in conversations that I sought out with people with experience
> implementing high-volume servers, both with threads and with async designs, my
> async skepticism met a lot of pushback. They consistently reported struggling
> with threaded designs and not being able to get performance under control until
> they went async. Big caveat: they were not using Rust - these were older designs
> in C++ and even C. But it jibes well with the other successful designs you see
> out there, like nginx and Elixir (which is used by WhatsApp, among others),
> which are all essentially async.
>
> So the purpose of these experiments was to see if I could isolate some of the
> sources of async's apparent advantages. It came down to memory consumption,
> creation time, and context switch time each having best-case
> order-of-magnitude advantages. Taken together, those advantages are beyond the
> point that I'm willing to call negligible. How often the best case actually
> arises is unclear, but one can argue that that, at least, is under the
> programmer's control, so the ceiling on how far implementation effort can get
> you is higher, in an async design.
>
> Ultimately, as far as this repo is concerned, you need to decide whether you
> trust your readers to understand both the value and the limitations of
> microbenchmarks. If you assume your readers are in Twitter mode---they're just
> going to glance at the headlines and come away with a binary, "async good, two
> legs bad" kind of conclusion---then maybe it's better not to publish
> microbenchmarks at all, because they're misleading. Reality is more sensitive to
> details. But I think the benefit of offering these microbenchmarks and the
> README's analysis to careful readers might(?) outweigh the harm done by the
> noise from careless readers, because I think the careful readers are more likely
> to use the material in a way that has lasting impact. The wind changes; the
> forest does not.
>
> The 2nd edition of Programming Rust (due out in June 2021) has a chapter on
> async that ends with a discussion of the rationale for async programming. It
> tries to dismiss some of the commonly heard bogus arguments, and present the
> advantages that async does have with the appropriate qualifications. It
> mentions tooling disadvantages. Generally, the chapter describes Rust's async
> implementation in a decent amount of detail, because we want our readers to be
> able to anticipate how it will perform and where it might help; the summary
> attempts to make clear what all that machinery can and cannot accomplish.

The only thing I'd add is that the measurements reported here for asynchronous
performance were taken of an implementation that uses `epoll`-style system
calls. The newer `io_uring`-style APIs seem radically different, and I'm curious
to see whether these might change the story here.
