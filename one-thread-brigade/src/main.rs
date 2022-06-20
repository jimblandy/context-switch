use std::time::Instant;
use std::os::unix::net::UnixStream;
use std::io::prelude::*;
use utils::{Stats, UsefulDuration};

struct Pipe {
    read: UnixStream,
    write: UnixStream,
}

fn pipe() -> Result<Pipe, std::io::Error> {
    let (read, write) = UnixStream::pair()?;
    Ok(Pipe { read, write })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    const NUM_TASKS: usize = 500;
    const NUM_WARMUP_REPS: usize = 5;
    const NUM_REPS: usize = 10000;

    let Pipe { read: mut upstream_read, write: mut first_write} = pipe()?;
    let mut pipes = Vec::new();
    for _i in 0..NUM_TASKS {
        let next_pipe = pipe()?;
        pipes.push(Pipe {
            read: upstream_read,
            write: next_pipe.write,
        });

        upstream_read = next_pipe.read;
    }

    fn brigade(pipes: &mut Vec<Pipe>) -> Result<(), std::io::Error> {
        let mut buf = [0_u8; 1];
        for pipe in pipes {
            pipe.read.read_exact(&mut buf)?;
            pipe.write.write_all(&buf)?;
        }
        Ok(())
    }

    let mut buf = [0_u8; 1];

    // Warm up.
    for _i in 0..NUM_WARMUP_REPS {
        first_write.write_all(b"*")?;
        brigade(&mut pipes)?;
        upstream_read.read(&mut buf)?;
    }

    let mut stats = Stats::new();
    for _i in 0..NUM_REPS {
        let start = Instant::now();
        first_write.write_all(b"*")?;
        brigade(&mut pipes)?;
        upstream_read.read(&mut buf)?;
        let end = Instant::now();

        stats.push(UsefulDuration::from(end - start).into());
    }

    println!("{} iterations, {} tasks, mean {} per iteration, stddev {} ({} per task per iter)",
             NUM_REPS, NUM_TASKS,
             UsefulDuration::from(stats.mean()),
             UsefulDuration::from(stats.population_stddev()),
             UsefulDuration::from(stats.mean() / NUM_TASKS as f64));

    Ok(())
}
