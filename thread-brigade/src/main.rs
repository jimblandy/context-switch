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
    const NUM_REPS: usize = 10000;

    let Pipe { read: mut upstream_read, write: mut first_write} = pipe()?;
    for _i in 0..NUM_TASKS {
        let next_pipe = pipe()?;
        let mut downstream_write = next_pipe.write;
        std::thread::spawn(move || -> Result<(), std::io::Error> {
            let mut buf = [0_u8; 1];

            loop {
                upstream_read.read_exact(&mut buf)?;
                downstream_write.write_all(&buf)?;
            }
        });
        upstream_read = next_pipe.read;
    }

    let mut stats = Stats::new();

    let mut buf = [0_u8; 1];
    for _i in 0..NUM_REPS {
        let start = Instant::now();
        first_write.write_all(b"*")?;
        upstream_read.read(&mut buf)?;
        let end = Instant::now();

        stats.push(UsefulDuration::from(end - start).into());
    }

    println!("{} iterations, mean {} per iteration, stddev {}",
             NUM_REPS,
             UsefulDuration::from(stats.mean()),
             UsefulDuration::from(stats.population_stddev()));

    Ok(())
}
