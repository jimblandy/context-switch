use docopt::Docopt;
use serde::Deserialize;
use std::process::Command;
use std::time::Instant;
use tokio::net::UnixStream;
use tokio::prelude::*;
use utils::{Stats, UsefulDuration};

const USAGE: &'static str = "
Microbenchmark of context switch overhead.

Create a chain of Rust asynchronous tasks connected together by pipes, each one
repeatedly reading a single byte from its upstream pipe and writing it to its
downstream pipe. One 'iteration' of the benchmark drops a byte in one end, and
measures the time required for it to come out the other end.

If `--measure COMMAND` is given, then the program runs `COMMAND` before exiting.
This gives an opportunity to measure the program's memory use. If `COMMAND`
contains the string `{pid}`, each occurrence is replaced with this program's
process ID.

Usage:
  thread-brigade [--threads N] [--iters N] [--warmups N] [--command COMMAND] [--quiet]

Options:
  --threads <N>     Number of async tasks (note: not OS threads). [default: 500]
  --iters <N>       Number of iterations to perform. [default: 10000]
  --warmups <N>     Number of warmup iterations to perform before benchmarking.
                    [default: 100]
  --command <CMD>   Command to run before exiting.
  --quiet           Don't print time measurements.
";

#[derive(Debug, Deserialize)]
struct Args {
    flag_threads: usize,
    flag_iters: usize,
    flag_warmups: usize,
    flag_command: Option<String>,
    flag_quiet: bool,
}

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
    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());

    if !args.flag_quiet {
        eprintln!("{} tasks, {} iterations:", args.flag_threads, args.flag_iters);
    }

    let Pipe { read: mut upstream_read, write: mut first_write} = pipe()?;
    for _i in 0..args.flag_threads {
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
    for _i in 0..args.flag_warmups {
        first_write.write_all(b"*").await?;
        upstream_read.read(&mut buf).await?;
    }

    let mut stats = Stats::new();
    for _i in 0..args.flag_iters {
        let start = Instant::now();
        first_write.write_all(b"*").await?;
        upstream_read.read(&mut buf).await?;
        let end = Instant::now();

        stats.push(UsefulDuration::from(end - start).into());
    }

    if !args.flag_quiet {
        eprintln!("mean {} per iteration, stddev {} ({} per task per iter)",
                  UsefulDuration::from(stats.mean()),
                  UsefulDuration::from(stats.population_stddev()),
                  UsefulDuration::from(stats.mean() / args.flag_threads as f64));
    }

    if let Some(command) = args.flag_command {
        let command = command.replace("{pid}", &std::process::id().to_string());
        let status = Command::new("sh")
            .arg("-c")
            .arg(command)
            .status()?;
        if !status.success() {
            Err(format!("child exited with status: {}", status))?;
        }
    }

    Ok(())
}
