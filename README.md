# Bullshark Fallback & Randomness beacon
[![rustc](https://img.shields.io/badge/rustc-1.51+-blue?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![license](https://img.shields.io/badge/license-Apache-blue.svg?style=flat-square)](LICENSE)

This repo provides an experimental implementation of the asynchronous fallback protocol of [Bullshark](https://arxiv.org/pdf/2201.05677.pdf). The code is however incomplete and there are currently no plans to maintain it.

The randomness beacon has been implemented to generate glocal coin for bullshark and tusk, also for beacon output. 
The crypto uses both elliptic cryptography and signature as pre-quantum way; Both lattice-based polynomial committment and dilithium signature as a post-quantum way.


## License
This software is licensed as [Apache 2.0](LICENSE).
