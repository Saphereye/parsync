# Parallel file synchronizer
The program aims to improve upon file transfer/copying speeds by leveraging multithreaded options.
Very loose "benchmarking" shows promise, with the following results

| File Size | rsync       | parsync       |
|-----------|-------------|---------------|
| 34.21 GiB | 1m 57s      | 27s

This project doesn't aim to replace rsync, but rather to provide a faster alternative for those who need it (like me).

