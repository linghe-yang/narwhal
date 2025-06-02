# Bullshark, Tusk & Randomness Beacon

[![rustc](https://img.shields.io/badge/rustc-1.85+-blue?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![python](https://img.shields.io/badge/python-3.9-blue?style=flat-square&logo=python&logoColor=white)](https://www.python.org/downloads/release/python-390/)
[![license](https://img.shields.io/badge/license-Apache-blue.svg?style=flat-square)](LICENSE)

This repository provides an implementation of Bullshark, Tusk, integrated with a Randomness Beacon. The codebase combines two DAG-based BFT-SMR protocols, Bullshark and Tusk, with Rubato, a high-throughput Randomness Beacon protocol. The codebase is designed to be small, efficient, and easy to benchmark and modify. It is not intended for production use but employs real cryptography (dalek, dilithum), networking (tokio), and storage (rocksdb).

## Quick Start

The core protocols are written in Rust, but all benchmarking scripts are written in Python and run with Fabric. The experiments were conducted on Ubuntu 20.04. To deploy and benchmark a testbed of 4 nodes on your local machine, clone the repo and install the Python dependencies:

```
$ git clone https://github.com/linghe-yang/narwhal.git
$ cd narwhal/benchmark
$ pip install -r requirements.txt
```

You also need to install Clang (required by rocksdb) and tmux (which runs all nodes and clients in the background). Finally, run a local benchmark using Fabric:

- For classic cryptography:

  ```
  $ fab local
  ```
- For post-quantum secure cryptography:

  ```
  $ fab local-pq
  ```

This command may take a long time the first time you run it (compiling Rust code in `release` mode may be slow). You can customize benchmark parameters in `fabfile.py`. To switch between Bullshark and Tusk, modify the `'protocol'` parameter in `fabfile.py`:

- `'protocol': 'dolphin'` for Bullshark's asynchronous version.
- `'protocol': 'tusk'` for Tusk.

To evaluate the Randomness Beacon results, set `'eval_beacon': True` in `fabfile.py`. When the benchmark terminates, it displays a summary of the execution similar to the one below:

```
-----------------------------------------
 SUMMARY:
-----------------------------------------
 + CONFIG:
 Faults: 0 node(s)
 Committee size: 4 node(s)
 Worker(s) per node: 1 worker(s)
 Collocate primary and workers: True
 Input rate: 50,000 tx/s
 Transaction size: 512 B
 Execution time: 20 s

 Header size: 1,000 B
 Max header delay: 200 ms
 GC depth: 500 round(s)
 Sync retry delay: 5,000 ms
 Sync retry nodes: 3 node(s)
 batch size: 500,000 B
 Max batch delay: 200 ms
 DAG leaders per epoch: 40
 Max beacon requests per epoch: 216

 + BEACON RESULTS:
 Primary-0 Beacon Output Rate: 238.566 beacons/s, Beacon Resource Generation Rate: 277.895 beacons/s, Gather Latency: 79 ms, Beacon Latency: 780 ms
 Primary-1 Beacon Output Rate: 238.566 beacons/s, Beacon Resource Generation Rate: 277.895 beacons/s, Gather Latency: 78 ms, Beacon Latency: 780 ms
 Primary-2 Beacon Output Rate: 238.590 beacons/s, Beacon Resource Generation Rate: 277.927 beacons/s, Gather Latency: 80 ms, Beacon Latency: 780 ms
 Primary-3 Beacon Output Rate: 238.566 beacons/s, Beacon Resource Generation Rate: 277.878 beacons/s, Gather Latency: 81 ms, Beacon Latency: 780 ms
 Beacon Equivocation Errors: 0
-----------------------------------------
```

## License

This software is licensed under Apache 2.0.