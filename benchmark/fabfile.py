# Copyright(C) Facebook, Inc. and its affiliates.
from fabric import task

from benchmark.local import LocalBench
from benchmark.logs import ParseError, LogParser
from benchmark.utils import Print
from benchmark.plot import Ploter, PlotError
from benchmark.instance import InstanceManager
from benchmark.remote import Bench, BenchError


@task
def local(ctx, debug=False):
    ''' Run benchmarks on localhost (fab local) '''
    bench_params = {
        'faults': 0,
        'nodes': 4,
        'workers': 1,
        'rate': 50_000,
        'tx_size': 512,
        'duration': 20,
        'protocol': 'dolphin', # dolphin or tusk
        'crypto': 'origin', # origin or post_quantum (need extra params)
        'avss_batch_size': 256,
        'leader_per_epoch': 40
    }
    node_params = {
        'timeout': 5_000,  # ms
        'header_size': 1_000,  # bytes
        'max_header_delay': 200,  # ms
        'gc_depth': 500,  # rounds
        'sync_retry_delay': 5_000,  # ms
        'sync_retry_nodes': 3,  # number of nodes
        'batch_size': 500_000,  # bytes
        'max_batch_delay': 200,  # ms
        'beacon_req_delay': 0, # ms
        'breeze_epoch_limit': 20,
        'eval_beacon': True # True: log beacon result; False: log consensus result
    }
    try:
        ret = LocalBench(bench_params, node_params).run(debug)
        print(ret.result())
    except BenchError as e:
        Print.error(e)

@task
def local_pq(ctx, debug=False):
    ''' Run benchmarks on localhost (fab local-pq)'''
    bench_params = {
        'faults': 0,
        'nodes': 4,
        'workers': 1,
        'rate': 50_000,
        'tx_size': 512,
        'duration': 20,
        'protocol': 'dolphin',
        'crypto': 'post_quantum',
        'avss_batch_size': 256,
        'leader_per_epoch': 40,
        "n": 16,
        "log_q": 32,
        "g": 1,
        "kappa": 16,
        "r": 2,
        "ell": 0
    }
    node_params = {
        'timeout': 5_000,  # ms
        'header_size': 50,  # bytes
        'max_header_delay': 1_000,  # ms
        'gc_depth': 100,  # rounds
        'sync_retry_delay': 5_000,  # ms
        'sync_retry_nodes': 3,  # number of nodes
        'batch_size': 500_000,  # bytes
        'max_batch_delay': 200,  # ms
        'beacon_req_delay': 0, # ms
        'breeze_epoch_limit': 20,
        'eval_beacon': True
    }
    try:
        ret = LocalBench(bench_params, node_params).run(debug)
        print(ret.result())
    except BenchError as e:
        Print.error(e)

@task
def create(ctx, nodes=4):
    ''' Create a testbed'''
    try:
        InstanceManager.make().create_instances(nodes)
    except BenchError as e:
        Print.error(e)

@task
def info(ctx):
    ''' Display connect information about all the available machines '''
    try:
        InstanceManager.make().print_info()
    except BenchError as e:
        Print.error(e)

@task
def destroy(ctx):
    ''' Destroy the testbed '''
    try:
        InstanceManager.make().terminate_instances()
    except BenchError as e:
        Print.error(e)


@task
def start(ctx, max=20):
    ''' Start at most `max` machines per data center '''
    try:
        InstanceManager.make().start_instances(max)
    except BenchError as e:
        Print.error(e)


@task
def stop(ctx):
    ''' Stop all machines '''
    try:
        InstanceManager.make().stop_instances()
    except BenchError as e:
        Print.error(e)

@task
def install(ctx): # unused
    ''' Install the codebase on all machines '''
    try:
        Bench(ctx).install()
    except BenchError as e:
        Print.error(e)


@task
def remote(ctx, debug=False, update=True, update_crs=True):
    ''' Run benchmarks on AWS '''
    bench_params = {
        'faults': 0,
        'nodes': [10],
        'workers': 1,
        'collocate': True,
        'rate': [200_000],
        'tx_size': 512,
        'duration': 240,
        'runs': 2,
        'protocol': 'dolphin',
        'crypto': 'origin',
        'avss_batch_size': 2432,
        'leader_per_epoch': 1200
    }
    node_params = {
        'timeout': 5_000,  # ms
        'header_size': 50,  # bytes
        'max_header_delay': 200,  # ms
        'gc_depth': 200,  # rounds
        'sync_retry_delay': 5_000,  # ms
        'sync_retry_nodes': 3,  # number of nodes
        'batch_size': 500_000,  # bytes
        'max_batch_delay': 200,  # ms
        'beacon_req_delay': 0, # ms
        'breeze_epoch_limit': 20,
        'eval_beacon': True
    }
    try:
        Bench(ctx).run(bench_params, node_params, debug, update,update_crs)
    except BenchError as e:
        Print.error(e)



@task
def remote_pq(ctx, debug=False, update=True, update_crs=True):
    ''' Run post quantum benchmarks on AWS '''
    bench_params = {
        'faults': 0,
        'nodes': [20],
        'workers': 1,
        'collocate': True,
        'rate': [200_000],
        'tx_size': 512,
        'duration': 30,
        'runs': 1,
        'protocol': 'dolphin',
        'crypto': 'post_quantum',
        'avss_batch_size': 2432,
        'leader_per_epoch': 1200,
        "n": 128,
        "log_q": 32,
        "g": 1,
        "kappa": 76,
        "r": 4,
        "ell": 0
    }
    node_params = {
        'timeout': 5_000,  # ms
        'header_size': 50,  # bytes
        'max_header_delay': 5_000,  # ms
        'gc_depth': 200,  # rounds
        'sync_retry_delay': 5_000,  # ms
        'sync_retry_nodes': 3,  # number of nodes
        'batch_size': 500_000,  # bytes
        'max_batch_delay': 200,  # ms
        'beacon_req_delay': 0, # ms
        'breeze_epoch_limit': 20,
        'eval_beacon': False
    }
    try:
        Bench(ctx).run(bench_params, node_params, debug, update, update_crs)
    except BenchError as e:
        Print.error(e)
@task
def plot(ctx):
    ''' Plot performance using the logs generated by "fab remote" '''
    plot_params = {
        'faults': [0],
        'nodes': [10, 20, 30, 50],
        'workers': [1],
        'collocate': True,
        'tx_size': 512,
        'protocol': ['bullshark', 'tusk'],
        'crypto': ['pq', 'npq'],
        'rate': 200000,
        'eval_beacon': True  # Set to False to skip Beacon plots
    }

    try:
        Ploter.plot(plot_params)
    except PlotError as e:
        Print.error(BenchError('Failed to plot performance', e))


@task
def kill(ctx):
    ''' Stop execution on all machines '''
    try:
        Bench(ctx).kill()
    except BenchError as e:
        Print.error(e)


@task
def logs(ctx):
    ''' Print a summary of the logs '''
    try:
        print(LogParser.process('./logs', faults='?').result())
    except ParseError as e:
        Print.error(BenchError('Failed to parse logs', e))