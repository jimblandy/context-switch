use std::sync::Arc;
use std::sync::{Condvar, Mutex};
use std::thread;
use std::time::Instant;
use utils::{Stats, UsefulDuration};

fn main() {
    const NUM_THREADS: usize = 20;
    const NUM_REPS: usize = 10000;

    let original_pair = Arc::new((Mutex::new(false), Condvar::new()));

    // First, create a bunch of threads, and then let them all exit. This should
    // allocate stacks for them, which I think NPTL will cache and reuse when a
    // new thread is created.
    let mut handles = Vec::with_capacity(NUM_THREADS);
    for _ in 0..NUM_THREADS {
        let pair = original_pair.clone();
        handles.push(thread::spawn(move || {
            let (lock, cvar) = &*pair;
            let mut guard = lock.lock().unwrap();
            while !*guard {
                guard = cvar.wait(guard).unwrap();
            }
        }));
    }

    // Tell them all to exit.
    {
        let (lock, cvar) = &*original_pair;
        let mut guard = lock.lock().unwrap();
        *guard = true;
        cvar.notify_all();
    }

    // Wait for them all to finish.
    for handle in handles.drain(..) {
        handle.join().unwrap();
    }

    // Measure the time to create a fresh batch, and let them exit.
    let mut stats = Stats::new();
    for _ in 0..NUM_REPS {
        let start = Instant::now();

        for _ in 0..NUM_THREADS {
            handles.push(thread::spawn(move || { }));
        }

        // Wait for them all to finish.
        for handle in handles.drain(..) {
            handle.join().unwrap();
        }

        let end = Instant::now();
        stats.push(UsefulDuration::from(end - start).into());
    }

    println!("{} iterations, {} threads, mean {} per iteration, stddev {} ({} per task per iter)",
             NUM_REPS, NUM_THREADS,
             UsefulDuration::from(stats.mean()),
             UsefulDuration::from(stats.population_stddev()),
             UsefulDuration::from(stats.mean() / NUM_THREADS as f64));
}
