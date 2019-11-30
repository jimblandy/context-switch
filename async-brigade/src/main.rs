use std::time::Instant;
use tokio::net::UnixStream;
use tokio::prelude::*;
use utils::{Stats, UsefulDuration};

struct Pipe {
    read: UnixStream,
    write: UnixStream,
}

fn pipe() -> Result<Pipe, std::io::Error> {
    let (read, write) = UnixStream::pair()?;
    Ok(Pipe { read, write })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    const NUM_TASKS: usize = 500;
    const NUM_WARMUP_REPS: usize = 100;
    const NUM_REPS: usize = 10000;

    let Pipe { read: mut upstream_read, write: mut first_write} = pipe()?;
    for _i in 0..NUM_TASKS {
        let next_pipe = pipe()?;
        let mut downstream_write = next_pipe.write;
        tokio::spawn(async move {
            let mut buf = [0_u8; 1];

            // Establish 'async' block's return type. Yeah.
            if false {
                return Ok::<(), std::io::Error>(());
            }

            loop {
                assert_eq!(upstream_read.read_exact(&mut buf).await?, 1);
                downstream_write.write_all(&buf).await?;
            }
        });
        upstream_read = next_pipe.read;
    }

    let mut buf = [0_u8; 1];

    // Warm up.
    for _i in 0..NUM_WARMUP_REPS {
        first_write.write_all(b"*").await?;
        upstream_read.read(&mut buf).await?;
    }

    let mut stats = Stats::new();
    for _i in 0..NUM_REPS {
        let start = Instant::now();
        first_write.write_all(b"*").await?;
        upstream_read.read(&mut buf).await?;
        let end = Instant::now();

        stats.push(UsefulDuration::from(end - start).into());
    }

    println!("{} iterations, mean {} per iteration, stddev {}",
             NUM_REPS,
             UsefulDuration::from(stats.mean()),
             UsefulDuration::from(stats.population_stddev()));

    Ok(())
}
