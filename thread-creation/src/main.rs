use std::thread;
use std::time::Instant;
use utils::{Stats, UsefulDuration};

fn main() {
    const NUM_THREADS: usize = 1000;
    const NUM_WARMUP: usize = 10;
    const NUM_REPS: usize = 100;

    struct StartedThread {
        start_time: Instant,
        handle: thread::JoinHandle<Instant>,
    }

    struct FinishedThread {
        start_time: Instant,
        end_time: Instant,
    }

    let mut started = Vec::with_capacity(NUM_THREADS);
    let mut finished = Vec::with_capacity(NUM_THREADS);

    eprintln!("{} threads, {} warmups, {} iterations:", NUM_THREADS, NUM_WARMUP, NUM_REPS);

    // Do a few warmup passes.
    for _warmup in 0..NUM_WARMUP {
        started.clear();
        finished.clear();

        for _ in 0..NUM_THREADS {
            let start_time = Instant::now();
            let handle = thread::spawn(move || { Instant::now() });
            started.push(StartedThread { start_time, handle });
        }

        finished.extend(started.drain(..)
                        .map(|StartedThread { start_time, handle }| {
                            let end_time = handle.join().unwrap();
                            FinishedThread { start_time, end_time }
                        }));
    }

    // Do the real passes.
    let mut creation_times = Stats::new();
    let mut started_times = Stats::new();
    for _rep in 0..NUM_REPS {
        started.clear();
        finished.clear();

        let start_creation = Instant::now();
        for _ in 0..NUM_THREADS {
            let start_time = Instant::now();
            let handle = thread::spawn(move || { Instant::now() });
            started.push(StartedThread { start_time, handle });
        }
        let end_creation = Instant::now();
        creation_times.push(UsefulDuration::from(end_creation - start_creation).into());

        finished.extend(started.drain(..)
                        .map(|StartedThread { start_time, handle }| {
                            let end_time = handle.join().unwrap();
                            FinishedThread { start_time, end_time }
                        }));

        started_times.extend(finished.iter()
                             .map(|FinishedThread { start_time, end_time }| {
                                 UsefulDuration::from(*end_time - *start_time).into()
                             }));
    }

    eprintln!("create a thread: mean {} per iter, stddev {} ({} per thread)",
              UsefulDuration::from(creation_times.mean()),
              UsefulDuration::from(creation_times.population_stddev()),
              UsefulDuration::from(creation_times.mean() / NUM_THREADS as f64));
    eprintln!("creation to body: mean {}, stddev {}",
              UsefulDuration::from(started_times.mean()),
              UsefulDuration::from(started_times.population_stddev()));
}
