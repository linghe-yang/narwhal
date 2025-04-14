# Bullshark Fallback & Randomness beacon
[![rustc](https://img.shields.io/badge/rustc-1.51+-blue?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![license](https://img.shields.io/badge/license-Apache-blue.svg?style=flat-square)](LICENSE)

This repo provides an experimental implementation of the asynchronous fallback protocol of [Bullshark](https://arxiv.org/pdf/2201.05677.pdf). The code is however incomplete and there are currently no plans to maintain it.

A randomness beacon has been implemented to generate global coins for Bullshark and Tusk, as well as for beacon output. The cryptographic framework employs both elliptic curve cryptography and ed25519-dalek signature as pre-quantum security measures, alongside lattice-based polynomial commitments and Dilithium signatures as post-quantum security mechanisms.



## License
This software is licensed as [Apache 2.0](LICENSE).
