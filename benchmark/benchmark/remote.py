# Copyright(C) Facebook, Inc. and its affiliates.
import os
from collections import OrderedDict
from fabric import Connection, ThreadingGroup as Group
from fabric.exceptions import GroupException
from paramiko import RSAKey
from paramiko.ssh_exception import PasswordRequiredException, SSHException
from os.path import basename, splitext
from time import sleep
from math import ceil
from copy import deepcopy
import subprocess
import json

from benchmark.config import Committee, Key, NodeParameters, BenchParameters, ConfigError
from benchmark.utils import BenchError, Print, PathMaker, progress_bar, distribute_rate
from benchmark.commands import CommandMaker
from benchmark.logs import LogParser, ParseError
from benchmark.instance import InstanceManager
from concurrent.futures import ThreadPoolExecutor


class FabricError(Exception):
    ''' Wrapper for Fabric exception with a meaningfull error message. '''

    def __init__(self, error):
        assert isinstance(error, GroupException)
        message = list(error.result.values())[-1]
        super().__init__(message)


class ExecutionError(Exception):
    pass


class Bench:
    def __init__(self, ctx):
        self.manager = InstanceManager.make()
        self.settings = self.manager.settings
        try:
            ctx.connect_kwargs.pkey = RSAKey.from_private_key_file(
                self.manager.settings.key_path
            )
            ctx.connect_kwargs.timeout = 10
            self.connect = ctx.connect_kwargs
        except (IOError, PasswordRequiredException, SSHException) as e:
            raise BenchError('Failed to load SSH key', e)

    def _check_stderr(self, output):
        if isinstance(output, dict):
            for x in output.values():
                if x.stderr:
                    raise ExecutionError(x.stderr)
        else:
            if output.stderr:
                raise ExecutionError(output.stderr)

    def install(self):
        Print.info('Installing rust and cloning the repo...')
        cmd = [
            'sudo apt-get update',
            'sudo apt-get -y upgrade',
            'sudo apt-get -y autoremove',

            # The following dependencies prevent the error: [error: linker `cc` not found].
            'sudo apt-get -y install build-essential',
            'sudo apt-get -y install cmake',

            # Install rust (non-interactive).
            'curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y',
            'source $HOME/.cargo/env',
            'rustup default stable',

            # This is missing from the Rocksdb installer (needed for Rocksdb).
            'sudo apt-get install -y clang',

            # Clone the repo.
            f'(git clone {self.settings.repo_url} || (cd {self.settings.repo_name} ; git pull))'
        ]
        hosts = self.manager.main_hosts(flat=True)
        try:
            g = Group(*hosts, user='ubuntu', connect_kwargs=self.connect)
            g.run(' && '.join(cmd), hide=True)
            Print.heading(f'Initialized testbed of {len(hosts)} nodes')
        except (GroupException, ExecutionError) as e:
            e = FabricError(e) if isinstance(e, GroupException) else e
            raise BenchError('Failed to install repo on testbed', e)

    def kill(self, hosts=[], delete_logs=False):
        assert isinstance(hosts, list)
        assert isinstance(delete_logs, bool)
        hosts = hosts if hosts else self.manager.hosts(flat=True)
        delete_logs = CommandMaker.clean_logs() if delete_logs else 'true'
        cmd = [delete_logs, f'({CommandMaker.kill()} || true)']
        try:
            g = Group(*hosts, user='ubuntu', connect_kwargs=self.connect)
            g.run(' && '.join(cmd), hide=True)
        except GroupException as e:
            raise BenchError('Failed to kill nodes', FabricError(e))

    def _select_hosts(self, bench_parameters):
        # Collocate the primary and its workers on the same machine.
        if bench_parameters.collocate:
            nodes = max(bench_parameters.nodes)

            # Ensure there are enough hosts.
            hosts = self.manager.hosts()
            if sum(len(x) for x in hosts.values()) < nodes:
                return []

            # Select the hosts in different data centers.
            ordered = zip(*hosts.values())
            ordered = [x for y in ordered for x in y]
            return ordered[:nodes]

        # Spawn the primary and each worker on a different machine. Each
        # authority runs in a single data center.
        else:
            primaries = max(bench_parameters.nodes)

            # Ensure there are enough hosts.
            hosts = self.manager.hosts()
            if len(hosts.keys()) < primaries:
                return []
            for ips in hosts.values():
                if len(ips) < bench_parameters.workers + 1:
                    return []

            # Ensure the primary and its workers are in the same region.
            selected = []
            for region in list(hosts.keys())[:primaries]:
                ips = list(hosts[region])[:bench_parameters.workers + 1]
                selected.append(ips)
            return selected

    def _background_run(self, host, command, log_file):
        name = splitext(basename(log_file))[0]
        cmd = f'tmux new -d -s "{name}" "{command} |& tee {log_file}"'
        c = Connection(host, user='ubuntu', connect_kwargs=self.connect)
        output = c.run(cmd, hide=True)
        self._check_stderr(output)

    def _update(self, hosts, collocate, protocol, crypto):
        if collocate:
            ips = list(set(hosts))
        else:
            ips = list(set([x for y in hosts for x in y]))

        Print.info(f'Testing SSH connectivity for {len(ips)} machines...')

        # Step 0: Test SSH connectivity
        g = Group(*ips, user='ubuntu', connect_kwargs=self.connect)
        test_cmd = 'echo "test"'
        failed_ips = []
        try:
            results = g.run(test_cmd, hide=True, warn=True, timeout=10)
            for ip, result in results.items():
                if result.exited != 0 or result.stderr:
                    failed_ips.append((ip, result.stderr or f'Non-zero exit code: {result.exited}'))
        except GroupException as e:
            for ip, result in e.result.items():
                if isinstance(result, Exception):
                    failed_ips.append((ip, str(result)))
                elif result.exited != 0 or result.stderr:
                    failed_ips.append((ip, result.stderr or f'Non-zero exit code: {result.exited}'))

        # Step 2: Check SSH test results
        if failed_ips:
            for ip, error in failed_ips:
                Print.warn(f'SSH connection failed on {ip}: {error}')
            raise BenchError(
                f'SSH connectivity test failed for {len(failed_ips)}/{len(ips)} instances',
                Exception('SSH connectivity issues')
            )
        Print.info(f'Successfully connected to all {len(ips)} instances')

        Print.info(
            f'Updating {len(ips)} machines with local binaries...'
        )

        # Step 1: Compile locally in ~/narwhal/node
        local_repo_path = os.path.expanduser(f'~/{self.settings.repo_name}')
        node_path = os.path.join(local_repo_path, 'node')

        try:
            # Compile locally
            Print.info('Compiling locally in ~/narwhal/node...')
            compile_cmd = CommandMaker.compile(protocol, crypto)
            subprocess.run(
                [compile_cmd], shell=True, check=True, cwd=node_path
            )
        except subprocess.CalledProcessError as e:
            raise BenchError(f'Failed to compile locally: {e.stderr}', e)
        except Exception as e:
            raise BenchError('Unexpected error during local compilation', e)

        # Recompile the gen_files crate
        if crypto == 'pq':
            cmd = CommandMaker.compile_gen_files_pq()
            subprocess.run(
                [cmd], shell=True, check=True
            )
        else:
            cmd = CommandMaker.compile_gen_files()
            subprocess.run(
                [cmd], shell=True, check=True
            )

        # Create alias for the client and nodes binary and gen_files.
        cmd = CommandMaker.alias_binaries(PathMaker.binary_path())
        subprocess.run([cmd], shell=True)

        # Step 2: Upload binaries to remote instances in parallel
        binary_dir = os.path.join(local_repo_path, 'target', 'release')
        remote_binary_dir = f'/home/ubuntu/{self.settings.repo_name}/target/release/'
        binaries = ['node', 'benchmark_client']

        def upload_binaries(ip):
            """Upload binaries to a single instance."""
            c = Connection(ip, user='ubuntu', connect_kwargs=self.connect)
            try:
                # Ensure remote directory exists
                c.run(f'mkdir -p {remote_binary_dir}', hide=True)
                # Upload each binary
                for binary in binaries:
                    local_path = os.path.join(binary_dir, binary)
                    remote_path = f'{remote_binary_dir}{binary}'
                    if os.path.exists(local_path):
                        c.put(local_path, remote_path)
                        # Set executable permissions
                        c.run(f'chmod +x {remote_path}', hide=True)
                # Create binary aliases
                c.run(
                    CommandMaker.alias_binaries_remote(f'./{self.settings.repo_name}/target/release/'),
                    hide=True
                )
            except Exception as e:
                raise BenchError(f'Failed to upload binaries to {ip}', e)

        Print.info(f'Uploading binaries to {len(ips)} instances in parallel...')
        with ThreadPoolExecutor(max_workers=10) as executor:
            futures = [
                executor.submit(upload_binaries, ip)
                for ip in ips
            ]
            # Use progress bar to track completion
            progress = progress_bar(futures, prefix='Uploading binaries:')
            for future in progress:
                future.result()  # Wait for each task to complete and raise any exceptions

        Print.info(f'Successfully updated {len(ips)} machines with local binaries')

    def _config(self, hosts, node_parameters, bench_parameters, update_crs):
        Print.info('Generating configuration files...')

        # Cleanup all local configuration files.
        cmd = CommandMaker.cleanup()
        subprocess.run([cmd], shell=True, stderr=subprocess.DEVNULL)

        # Generate configuration files.
        keys = []
        key_files = [PathMaker.key_file(i) for i in range(len(hosts))]
        for filename in key_files:
            cmd = CommandMaker.generate_key(filename).split()
            subprocess.run(cmd, check=True)
            keys += [Key.from_file(filename)]

        names = [x.name for x in keys]

        if bench_parameters.collocate:
            workers = bench_parameters.workers
            addresses = OrderedDict(
                (x, [y] * (workers + 1)) for x, y in zip(names, hosts)
            )
        else:
            addresses = OrderedDict(
                (x, y) for x, y in zip(names, hosts)
            )
        committee = Committee(addresses, self.settings.base_port)
        committee.print(PathMaker.committee_file())

        # Generate crs file
        if update_crs:
            if bench_parameters.crypto == 'pq':
                cmd = CommandMaker.generate_crs_q(
                    bench_parameters.n, bench_parameters.log_q, bench_parameters.g,
                    bench_parameters.kappa, bench_parameters.r, bench_parameters.ell
                ).split()
                subprocess.run(cmd, check=True)
            else:
                fault_tolerance = (len(hosts) - 1) // 3
                cmd = CommandMaker.generate_crs(fault_tolerance).split()
                subprocess.run(cmd, check=True)

        node_parameters.print(PathMaker.parameters_file())

        # Cleanup all nodes and upload configuration files in parallel.
        names = names[:len(names) - bench_parameters.faults]

        def upload_config(name, i):
            """Upload configuration files to a single instance."""
            for ip in committee.ips(name):
                with Connection(ip, user='ubuntu', connect_kwargs=self.connect) as c:
                    if update_crs:
                        c.run(f'{CommandMaker.cleanup()} || true', hide=True)
                    else:
                        c.run(f'{CommandMaker.cleanup_exp_crs()} || true', hide=True)
                    c.put(PathMaker.committee_file(), '.')
                    c.put(PathMaker.key_file(i), '.')
                    c.put(PathMaker.parameters_file(), '.')
                    if update_crs:
                        c.put(PathMaker.crs_file(), '.')

        Print.info(f'Uploading config files to {len(names)} instances in parallel...')
        with ThreadPoolExecutor(max_workers=10) as executor:
            futures = [
                executor.submit(upload_config, name, i)
                for i, name in enumerate(names)
            ]
            # Use progress bar to track completion.
            progress = progress_bar(futures, prefix='Uploading config files:')
            for future in progress:
                future.result()  # Wait for each task to complete and raise any exceptions.

        return committee

    def _run_single(self, rate, committee, bench_parameters, debug=False):
        faults = bench_parameters.faults

        # Kill any potentially unfinished run and delete logs.
        hosts = committee.ips()
        self.kill(hosts=hosts, delete_logs=True)

        def run_parallel(tasks):
            """Execute a list of (host, cmd, log_file) tasks in parallel."""
            with ThreadPoolExecutor(max_workers=len(hosts)) as executor:
                futures = [
                    executor.submit(self._background_run, host, cmd, log_file)
                    for host, cmd, log_file in tasks
                ]
                # Wait for all tasks to complete and collect any errors
                for future in futures:
                    try:
                        future.result()  # Raises any exceptions from _background_run
                    except ExecutionError as e:
                        Print.error(f"Error in background run: {str(e)}")
                        raise

        # Run the clients in parallel
        Print.info('Booting clients...')
        workers_addresses = committee.workers_addresses(faults)
        rate_shares = distribute_rate(rate, committee.workers())  # Get list of rate_shares

        client_tasks = []
        rate_share_index = 0  # To track which rate_share to assign
        for i, addresses in enumerate(workers_addresses):
            for (id, address) in addresses:
                if rate_share_index >= len(rate_shares):
                    raise ValueError("More clients than rate_shares assigned")
                host = Committee.ip(address)
                cmd = CommandMaker.run_client(
                    address,
                    bench_parameters.tx_size,
                    rate_shares[rate_share_index],  # Use specific rate_share
                    [x for y in workers_addresses for _, x in y]
                )
                log_file = PathMaker.client_log_file(i, id)
                client_tasks.append((host, cmd, log_file))
                rate_share_index += 1

        run_parallel(client_tasks)

        # Run the primaries in parallel
        Print.info('Booting primaries...')
        primary_tasks = []
        for i, address in enumerate(committee.primary_addresses(faults)):
            host = Committee.ip(address)
            cmd = CommandMaker.run_primary(
                PathMaker.key_file(i),
                PathMaker.committee_file(),
                PathMaker.db_path(i),
                PathMaker.crs_file(),
                PathMaker.parameters_file(),
                bench_parameters.avss_batch_size,
                bench_parameters.leader_per_epoch,
                debug=debug
            )
            log_file = PathMaker.primary_log_file(i)
            primary_tasks.append((host, cmd, log_file))
        run_parallel(primary_tasks)
        # Sleep if crypto is 'pq'
        if bench_parameters.crypto == 'pq':
            secret_size = bench_parameters.n * bench_parameters.kappa
            slag = 2 * (secret_size / 400 + secret_size / 4000 * committee.size())
            Print.info(f'Sleeping for {slag} seconds...') # wait for epoch 0's complete.
            sleep(slag)
        else:
            slag = 2 * (bench_parameters.avss_batch_size * 5 / 1000)
            Print.info(f'Sleeping for {slag} seconds...')
            sleep(slag)

        # Run the workers in parallel
        Print.info('Booting workers...')
        worker_tasks = []
        for i, addresses in enumerate(workers_addresses):
            for (id, address) in addresses:
                host = Committee.ip(address)
                cmd = CommandMaker.run_worker(
                    PathMaker.key_file(i),
                    PathMaker.committee_file(),
                    PathMaker.db_path(i, id),
                    PathMaker.parameters_file(),
                    id,  # The worker's id.
                    debug=debug
                )
                log_file = PathMaker.worker_log_file(i, id)
                worker_tasks.append((host, cmd, log_file))
        run_parallel(worker_tasks)

        # Wait for all transactions to be processed
        duration = bench_parameters.duration
        for _ in progress_bar(range(20), prefix=f'Running benchmark ({duration} sec):'):
            sleep(ceil(duration / 20))
        self.kill(hosts=hosts, delete_logs=False)



    def _logs(self, committee, faults):
        # Delete local logs (if any).
        cmd = CommandMaker.clean_logs()
        subprocess.run([cmd], shell=True, stderr=subprocess.DEVNULL)

        def download_primary_log(address, i):
            """Download primary log for a single address."""
            host = Committee.ip(address)
            with Connection(host, user='ubuntu', connect_kwargs=self.connect) as c:
                c.get(
                    PathMaker.primary_log_file(i),
                    local=PathMaker.primary_log_file(i)
                )

        def download_worker_logs(addresses, i):
            """Download client and worker logs for a single worker instance."""
            for id, address in addresses:
                host = Committee.ip(address)
                with Connection(host, user='ubuntu', connect_kwargs=self.connect) as c:
                    c.get(
                        PathMaker.client_log_file(i, id),
                        local=PathMaker.client_log_file(i, id)
                    )
                    c.get(
                        PathMaker.worker_log_file(i, id),
                        local=PathMaker.worker_log_file(i, id)
                    )

        # Download primary logs in parallel.
        primary_addresses = committee.primary_addresses(faults)
        Print.info(f'Downloading {len(primary_addresses)} primary logs in parallel...')
        with ThreadPoolExecutor(max_workers=10) as executor:
            futures = [
                executor.submit(download_primary_log, address, i)
                for i, address in enumerate(primary_addresses)
            ]
            # Use progress bar to track completion.
            progress = progress_bar(futures, prefix='Downloading primaries logs:')
            for future in progress:
                future.result()  # Wait for each task to complete and raise any exceptions.

        # Download worker logs in parallel.
        workers_addresses = committee.workers_addresses(faults)
        Print.info(f'Downloading {len(workers_addresses)} worker logs in parallel...')
        with ThreadPoolExecutor(max_workers=10) as executor:
            futures = [
                executor.submit(download_worker_logs, addresses, i)
                for i, addresses in enumerate(workers_addresses)
            ]
            # Use progress bar to track completion.
            progress = progress_bar(futures, prefix='Downloading workers logs:')
            for future in progress:
                future.result()  # Wait for each task to complete and raise any exceptions.

        # Parse logs and return the parser.
        Print.info('Parsing logs and computing performance...')
        return LogParser.process(PathMaker.logs_path(), faults=faults)

    def run(self, bench_parameters_dict, node_parameters_dict, debug=True, update=True, update_crs=True):
        assert isinstance(debug, bool)
        Print.heading('Starting remote benchmark')
        try:
            bench_parameters = BenchParameters(bench_parameters_dict)
            node_parameters = NodeParameters(node_parameters_dict)
        except ConfigError as e:
            raise BenchError('Invalid nodes or bench parameters', e)

        # Select which hosts to use.
        selected_hosts = self._select_hosts(bench_parameters)
        if not selected_hosts:
            Print.warn('There are not enough instances available')
            return

        # Update nodes.
        if update:
            try:
                self._update(
                    selected_hosts,
                    bench_parameters.collocate,
                    bench_parameters.protocol,
                    bench_parameters.crypto
                )
            except (GroupException, ExecutionError) as e:
                e = FabricError(e) if isinstance(e, GroupException) else e
                raise BenchError('Failed to update nodes', e)

        # Upload all configuration files.
        try:
            committee = self._config(
                selected_hosts, node_parameters, bench_parameters, update_crs
            )
        except (subprocess.SubprocessError, GroupException) as e:
            e = FabricError(e) if isinstance(e, GroupException) else e
            raise BenchError('Failed to configure nodes', e)

        # Run benchmarks.
        for n in bench_parameters.nodes:
            committee_copy = deepcopy(committee)
            committee_copy.remove_nodes(committee.size() - n)

            for r in bench_parameters.rate:
                Print.heading(f'\nRunning {n} nodes (input rate: {r:,} tx/s)')

                # Run the benchmark.
                for i in range(bench_parameters.runs):
                    Print.heading(f'Run {i + 1}/{bench_parameters.runs}')
                    try:
                        self._run_single(
                            r, committee_copy, bench_parameters, debug
                        )

                        faults = bench_parameters.faults
                        logger = self._logs(committee_copy, faults)
                        logger.print(PathMaker.result_file(
                            faults,
                            n,
                            bench_parameters.workers,
                            bench_parameters.collocate,
                            r,
                            bench_parameters.tx_size,
                            bench_parameters.protocol,
                            bench_parameters.crypto,
                            node_parameters.json['eval_beacon']
                        ))
                    except (subprocess.SubprocessError, GroupException, ParseError) as e:
                        self.kill(hosts=selected_hosts)
                        if isinstance(e, GroupException):
                            e = FabricError(e)
                        Print.error(BenchError('Benchmark failed', e))
                        continue
