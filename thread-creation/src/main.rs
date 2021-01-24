use docopt::Docopt;
use serde::Deserialize;
use std::thread;
use std::time::Instant;
use utils::{Stats, UsefulDuration};

const USAGE: &'static str = "
Microbenchmark of task creation overhead.

Spawn a given number of asynchronous tasks. Measure how long it takes for the
spawning process to spawn all the tasks, and how long it takes a spawned task to
begin execution.

Usage:
  task-creation [--tasks N] [--iters N] [--warmups N]

Options:
  --tasks <N>       Number of tasks. [default: 1000]
  --iters <N>       Number of iterations to perform. [default: 100]
  --warmups <N>     Number of warmup iterations to perform before benchmarking.
                    [default: 10]
";

#[derive(Debug, Deserialize)]
struct Args {
    flag_tasks: usize,
    flag_iters: usize,
    flag_warmups: usize,
}

fn main() {
    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());

    struct StartedTask {
        start_time: Instant,
        handle: thread::JoinHandle<Instant>,
    }

    struct FinishedTask {
        start_time: Instant,
        end_time: Instant,
    }

    let mut started = Vec::with_capacity(args.flag_tasks);
    let mut finished = Vec::with_capacity(args.flag_tasks);

    eprintln!("{} tasks, {} warmups, {} iterations:", args.flag_tasks, args.flag_warmups, args.flag_iters);

    // Do a few warmup passes.
    for _warmup in 0..args.flag_warmups {
        started.clear();
        finished.clear();

        for _ in 0..args.flag_tasks {
            let start_time = Instant::now();
            let handle = thread::spawn(move || { Instant::now() });
            started.push(StartedTask { start_time, handle });
        }

        finished.extend(started.drain(..)
                        .map(|StartedTask { start_time, handle }| {
                            let end_time = handle.join().unwrap();
                            FinishedTask { start_time, end_time }
                        }));
    }

    // Do the real passes.
    let mut creation_times = Stats::new();
    let mut started_times = Stats::new();
    for _rep in 0..args.flag_iters {
        started.clear();
        finished.clear();

        let start_creation = Instant::now();
        for _ in 0..args.flag_tasks {
            let start_time = Instant::now();
            let handle = thread::spawn(move || { Instant::now() });
            started.push(StartedTask { start_time, handle });
        }
        let end_creation = Instant::now();
        creation_times.push(UsefulDuration::from(end_creation - start_creation).into());

        finished.extend(started.drain(..)
                        .map(|StartedTask { start_time, handle }| {
                            let end_time = handle.join().unwrap();
                            FinishedTask { start_time, end_time }
                        }));

        started_times.extend(finished.iter()
                             .map(|FinishedTask { start_time, end_time }| {
                                 UsefulDuration::from(*end_time - *start_time).into()
                             }));
    }

    eprintln!("create a task: mean {} per iter, stddev {} ({} per task)",
              UsefulDuration::from(creation_times.mean()),
              UsefulDuration::from(creation_times.population_stddev()),
              UsefulDuration::from(creation_times.mean() / args.flag_tasks as f64));
    eprintln!("creation to body: mean {}, stddev {}",
              UsefulDuration::from(started_times.mean()),
              UsefulDuration::from(started_times.population_stddev()));
}
