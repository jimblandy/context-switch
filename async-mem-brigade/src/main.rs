use std::time::Instant;
use tokio::sync::mpsc;
use utils::{Stats, UsefulDuration};

struct Pipe {
    read: mpsc::Receiver<usize>,
    write: mpsc::Sender<usize>,
}

fn pipe() -> Result<Pipe, std::io::Error> {
    let (write, read) = mpsc::channel(1);
    Ok(Pipe { read, write })
}

#[tokio::main(basic_scheduler)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    const NUM_TASKS: usize = 500;
    const NUM_WARMUP_REPS: usize = 5;
    const NUM_REPS: usize = 10000;

    let Pipe { read: mut upstream_read, write: mut first_write} = pipe()?;
    for _i in 0..NUM_TASKS {
        let next_pipe = pipe()?;
        let mut downstream_write = next_pipe.write;
        tokio::spawn(async move {
            // Establish 'async' block's return type. Yeah.
            if false {
                return Ok::<(), mpsc::error::SendError<usize>>(());
            }

            loop {
                let n = upstream_read.recv().await.unwrap();
                downstream_write.send(n + 1).await?;
            }
        });
        upstream_read = next_pipe.read;
    }

    // Warm up.
    for _i in 0..NUM_WARMUP_REPS {
        first_write.send(0).await?;
        assert_eq!(upstream_read.recv().await, Some(NUM_TASKS));
    }

    let mut stats = Stats::new();
    for _i in 0..NUM_REPS {
        let start = Instant::now();
        first_write.send(0).await?;
        assert_eq!(upstream_read.recv().await, Some(NUM_TASKS));
        let end = Instant::now();

        stats.push(UsefulDuration::from(end - start).into());
    }

    println!("{} iterations, {} tasks, mean {} per iteration, stddev {} ({} per task per iter)",
             NUM_REPS, NUM_TASKS,
             UsefulDuration::from(stats.mean()),
             UsefulDuration::from(stats.population_stddev()),
             UsefulDuration::from(stats.mean() / NUM_TASKS as f64));

    // Otherwise, Tokio blocks waiting for other tasks to finish. I don't want
    // to risk introducing noise by adding shutdown logic to them, so just exit
    // the entire process.
    std::process::exit(0);
}
