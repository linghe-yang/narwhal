# Copyright(C) Facebook, Inc. and its affiliates.
from os.path import join

from benchmark.utils import PathMaker


class CommandMaker:

    @staticmethod
    def cleanup():
        return (
            f'rm -r .db-* ; rm .*.json ; mkdir -p {PathMaker.results_path()}'
        )

    @staticmethod
    def clean_logs():
        return f'rm -r {PathMaker.logs_path()} ; mkdir -p {PathMaker.logs_path()}'

    @staticmethod
    def compile(protocol='tusk', crypto='origin'):
        protocol = '' if protocol == 'tusk' else protocol
        crypto = '' if crypto == 'origin' else crypto
        return f'cargo build --quiet --release --features "benchmark {crypto} {protocol}"'

    @staticmethod
    def compile_gen_files():
        return f'cargo build --quiet --release --package gen_files --features "benchmark"'
    @staticmethod
    def compile_gen_files_pq():
        return f'cargo build --quiet --release --package gen_files --features "benchmark pq"'
    @staticmethod
    def generate_key(filename):
        assert isinstance(filename, str)
        return f'./node generate_keys --filename {filename}'

    @staticmethod
    def generate_crs(faults):
        return f'./gen_files generate_crs --fault_tolerance {faults}'

    @staticmethod
    def generate_crs_q(n, log_q, g, kappa, r,ell):
        return f'./gen_files generate_crs --n {n} --log_q {log_q} --g {g} --kappa {kappa} --r {r} --ell {ell}'

    @staticmethod
    def run_primary(keys, committee, store, crs, parameters, avss_batch_size, leader_per_epoch, debug=False):
        assert isinstance(keys, str)
        assert isinstance(committee, str)
        assert isinstance(parameters, str)
        assert isinstance(crs, str)
        assert isinstance(debug, bool)
        v = '-vvv' if debug else '-vv'
        return (f'./node {v} run --keys {keys} --committee {committee} '
                f'--store {store} --parameters {parameters} primary --crs {crs} --bs {avss_batch_size} --le {leader_per_epoch}')

    @staticmethod
    def run_worker(keys, committee, store, parameters, id, debug=False):
        assert isinstance(keys, str)
        assert isinstance(committee, str)
        assert isinstance(parameters, str)
        assert isinstance(debug, bool)
        v = '-vvv' if debug else '-vv'
        return (f'./node {v} run --keys {keys} --committee {committee} '
                f'--store {store} --parameters {parameters} worker --id {id}')

    @staticmethod
    def run_client(address, size, rate, nodes):
        assert isinstance(address, str)
        assert isinstance(size, int) and size > 0
        assert isinstance(rate, int) and rate >= 0
        assert isinstance(nodes, list)
        assert all(isinstance(x, str) for x in nodes)
        nodes = f'--nodes {" ".join(nodes)}' if nodes else ''
        return f'./benchmark_client {address} --size {size} --rate {rate} {nodes}'

    @staticmethod
    def kill():
        return 'tmux kill-server'

    @staticmethod
    def alias_binaries_remote(origin):
        assert isinstance(origin, str)
        node, client = join(origin, 'node'), join(origin, 'benchmark_client')
        return f'rm node ; rm benchmark_client ; ln -s {node} . ; ln -s {client} .'


    @staticmethod
    def alias_binaries(origin):
        assert isinstance(origin, str)
        node, client, gen_files = join(origin, 'node'), join(origin, 'benchmark_client'), join(origin, 'gen_files')
        return f'rm node ; rm benchmark_client; rm gen_files ; ln -s {node} . ; ln -s {client} .; ln -s {gen_files} .'
