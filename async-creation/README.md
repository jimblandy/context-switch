# async-creation: Measure cost of async task creation

This microbenchmark tries to measure how long it takes to spawn an asynchronous
task. It spawns a given number of asynchronous tasks. Measure how long it takes
for the spawning process to spawn all the tasks, and how long it takes a spawned
task to begin execution.
