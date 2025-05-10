# Copyright(C) Facebook, Inc. and its affiliates.
from datetime import datetime
from glob import glob
from multiprocessing import Pool
from os.path import join
from re import findall, search
from statistics import mean

from benchmark.utils import Print


class ParseError(Exception):
    pass


class LogParser:
    def __init__(self, clients, primaries, workers, faults=0):
        inputs = [clients, primaries, workers]
        assert all(isinstance(x, list) for x in inputs)
        assert all(isinstance(x, str) for y in inputs for x in y)
        assert all(x for x in inputs)

        self.faults = faults
        if isinstance(faults, int):
            self.committee_size = len(primaries) + int(faults)
            self.workers =  len(workers) // len(primaries)
        else:
            self.committee_size = '?'
            self.workers = '?'

        # Parse the clients logs.
        try:
            with Pool() as p:
                results = p.map(self._parse_clients, clients)
        except (ValueError, IndexError, AttributeError) as e:
            raise ParseError(f'Failed to parse clients\' logs: {e}')
        self.size, self.rate, self.start, misses, self.sent_samples \
            = zip(*results)
        self.misses = sum(misses)

        # Parse the primaries logs.
        try:
            with Pool() as p:
                results = p.map(self._parse_primaries, primaries)
        except (ValueError, IndexError, AttributeError) as e:
            raise ParseError(f'Failed to parse nodes\' logs: {e}')
        proposals, commits, self.configs, primary_ips, beacons, start_times, resources, shares, gathers = zip(*results)
        self.proposals = self._merge_results([x.items() for x in proposals])
        self.commits = self._merge_results([x.items() for x in commits])
        self.beacons_per_primary = beacons
        self.start_times = start_times
        self.commits_per_primary = commits  # Store individual commits for each primary
        self.resources_per_primary = resources  # Store resource data
        self.shares_per_primary = shares
        self.gathers_per_primary = gathers

        # Parse the workers logs.
        try:
            with Pool() as p:
                results = p.map(self._parse_workers, workers)
        except (ValueError, IndexError, AttributeError) as e:
            raise ParseError(f'Failed to parse workers\' logs: {e}')
        sizes, self.received_samples, workers_ips = zip(*results)
        self.sizes = {
            k: v for x in sizes for k, v in x.items() if k in self.commits
        }

        # Determine whether the primary and the workers are collocated.
        self.collocate = set(primary_ips) == set(workers_ips)

        # Check whether clients missed their target rate.
        if self.misses != 0:
            Print.warn(
                f'Clients missed their target rate {self.misses:,} time(s)'
            )

    def _merge_results(self, input):
        # Keep the earliest timestamp.
        merged = {}
        for x in input:
            for k, v in x:
                if not k in merged or merged[k] > v:
                    merged[k] = v
        return merged

    def _parse_clients(self, log):
        if search(r'Error', log) is not None:
            raise ParseError('Client(s) panicked')

        size = int(search(r'Transactions size: (\d+)', log).group(1))
        rate = int(search(r'Transactions rate: (\d+)', log).group(1))

        tmp = search(r'\[(.*Z) .* Start ', log).group(1)
        start = self._to_posix(tmp)

        misses = len(findall(r'rate too high', log))

        tmp = findall(r'\[(.*Z) .* sample transaction (\d+)', log)
        samples = {int(s): self._to_posix(t) for t, s in tmp}

        return size, rate, start, misses, samples

    def _parse_primaries(self, log):
        if search(r'(?:panicked|Error)', log) is not None:
            raise ParseError('Primary(s) panicked')

        tmp = findall(r'\[(.*Z) .* Created B\d+\([^ ]+\) -> ([^ ]+=)', log)
        tmp = [(d, self._to_posix(t)) for t, d in tmp]
        proposals = self._merge_results([tmp])

        tmp = findall(r'\[(.*Z) .* Committed B\d+\([^ ]+\) -> ([^ ]+=)', log)
        tmp = [(d, self._to_posix(t)) for t, d in tmp]
        commits = self._merge_results([tmp])

        configs = {
            'header_size': int(
                search(r'Header size .* (\d+)', log).group(1)
            ),
            'max_header_delay': int(
                search(r'Max header delay .* (\d+)', log).group(1)
            ),
            'gc_depth': int(
                search(r'Garbage collection depth .* (\d+)', log).group(1)
            ),
            'sync_retry_delay': int(
                search(r'Sync retry delay .* (\d+)', log).group(1)
            ),
            'sync_retry_nodes': int(
                search(r'Sync retry nodes .* (\d+)', log).group(1)
            ),
            'batch_size': int(
                search(r'Batch size .* (\d+)', log).group(1)
            ),
            'max_batch_delay': int(
                search(r'Max batch delay .* (\d+)', log).group(1)
            ),
            'dag_waves_per_epoch': int(
                search(r'Beacons for leader election per epoch:(\d+)', log).group(1)
            ),
            'beacons_per_epoch': int(
                search(r'Beacons for output per epoch:(\d+)', log).group(1)
            )
        }

        ip = search(r'booted on (\d+.\d+.\d+.\d+)', log).group(1)
        # add beacon
        beacon_pattern = r'\[(.*Z) .* Beacon output for epoch:(\d+) index:(\d+) is (\d+)'
        tmp = findall(beacon_pattern, log)
        beacons = [
            {
                'timestamp': self._to_posix(t),
                'epoch': int(e),
                'index': int(i),
                'value': int(v)
            }
            for t, e, i, v in tmp
        ]
        start_pattern = r'\[(.*Z) .* Starting Consensus\.\.\.'
        start_match = search(start_pattern, log)
        start_time = self._to_posix(start_match.group(1)) if start_match else None

        # Parse beacon resources
        resource_pattern = r'\[(.*Z) .* Common core for epoch:(\d+) decided\. Beacon resource add:(\d+)'
        tmp = findall(resource_pattern, log)
        resources = [
            {
                'timestamp': self._to_posix(t),
                'epoch': int(e),
                'resource': int(r)
            }
            for t, e, r in tmp
        ]

        # Parse share commands
        share_pattern = r'\[(.*Z) .* Share command send for epoch:(\d+)'
        tmp = findall(share_pattern, log)
        shares = [
            {
                'timestamp': self._to_posix(t),
                'epoch': int(e)
            }
            for t, e in tmp
        ]

        # Parse share commands
        gather_pattern = r'\[(.*Z) .* Breeze Certificate received for epoch:(\d+)'
        tmp = findall(gather_pattern, log)
        gathers = [
            {
                'timestamp': self._to_posix(t),
                'epoch': int(e)
            }
            for t, e in tmp
        ]

        return proposals, commits, configs, ip, beacons, start_time, resources, shares, gathers

    def _parse_workers(self, log):
        if search(r'(?:panic|Error)', log) is not None:
            raise ParseError('Worker(s) panicked')

        tmp = findall(r'Batch ([^ ]+) contains (\d+) B', log)
        sizes = {d: int(s) for d, s in tmp}

        tmp = findall(r'Batch ([^ ]+) contains sample tx (\d+)', log)
        samples = {int(s): d for d, s in tmp}

        ip = search(r'booted on (\d+.\d+.\d+.\d+)', log).group(1)

        return sizes, samples, ip

    def _to_posix(self, string):
        x = datetime.fromisoformat(string.replace('Z', '+00:00'))
        return datetime.timestamp(x)

    def _consensus_throughput(self):
        if not self.commits:
            return 0, 0, 0
        start, end = min(self.proposals.values()), max(self.commits.values())
        duration = end - start
        bytes = sum(self.sizes.values())
        bps = bytes / duration
        tps = bps / self.size[0]
        return tps, bps, duration

    def _consensus_latency(self):
        latency = [c - self.proposals[d] for d, c in self.commits.items()]
        return mean(latency) if latency else 0

    def _end_to_end_throughput(self):
        if not self.commits:
            return 0, 0, 0
        start, end = min(self.start), max(self.commits.values())
        duration = end - start
        bytes = sum(self.sizes.values())
        bps = bytes / duration
        tps = bps / self.size[0]
        return tps, bps, duration

    def _end_to_end_latency(self):
        latency = []
        for sent, received in zip(self.sent_samples, self.received_samples):
            for tx_id, batch_id in received.items():
                if batch_id in self.commits:
                    assert tx_id in sent  # We receive txs that we sent.
                    start = sent[tx_id]
                    end = self.commits[batch_id]
                    latency += [end-start]
        return mean(latency) if latency else 0

    # def _beacon_rate_per_primary(self):
    #     """calculate beacon rate for each primary"""
    #     rates = []
    #     for idx, beacons in enumerate(self.beacons_per_primary):
    #         if not beacons:
    #             rates.append((f'Primary-{idx}', 0))
    #             continue
    #         timestamps = [b['timestamp'] for b in beacons]
    #         duration = max(timestamps) - min(timestamps) if timestamps else 0
    #         count = len(beacons)
    #         rate = count / duration if duration > 0 else 0
    #         rates.append((f'Primary-{idx}', rate))
    #     return rates
    def _beacon_rate_per_primary(self):
        """Calculate beacon rate for each primary using consensus start and last commit/beacon time"""
        rates = []
        for idx, (beacons, start_time) in enumerate(zip(self.beacons_per_primary, self.start_times)):
            if not beacons or start_time is None:
                rates.append((f'Primary-{idx}', 0))
                continue

            # Get beacon timestamps
            beacon_timestamps = [b['timestamp'] for b in beacons]
            # Get commit timestamps
            commit_timestamps = list(self.commits_per_primary[idx].values())
            # Take the latest timestamp from commits or beacons
            end_time = max(
                max(beacon_timestamps) if beacon_timestamps else float('-inf'),
                max(commit_timestamps) if commit_timestamps else float('-inf')
            )
            duration = end_time - start_time if end_time != float('-inf') else 0
            count = len(beacons)
            rate = count / duration if duration > 0 else 0
            rates.append((f'Primary-{idx}', rate))
        return rates

    def _beacon_resource_rate_per_primary(self):
        """Calculate beacon resource generation rate for each primary"""
        rates = []
        for idx, resources in enumerate(self.resources_per_primary):
            if not resources:
                rates.append((f'Primary-{idx}', 0))
                continue
            timestamps = [r['timestamp'] for r in resources]
            total_resources = sum(r['resource'] for r in resources)
            duration = max(timestamps) - min(timestamps) if timestamps else 0
            rate = total_resources / duration if duration > 0 else 0
            rates.append((f'Primary-{idx}', rate))
        return rates

    def _beacon_latency_per_primary(self):
        """Calculate average beacon latency for each primary in milliseconds"""
        latencies = []
        for idx, (shares, resources) in enumerate(zip(self.shares_per_primary, self.resources_per_primary)):
            if not shares or not resources:
                latencies.append((f'Primary-{idx}', 'NULL'))
                continue
            # Build epoch-to-timestamp mappings
            share_times = {s['epoch']: s['timestamp'] for s in shares}
            resource_times = {r['epoch']: r['timestamp'] for r in resources}
            # Calculate valid latency for matching epochs
            valid_latencies = []
            for epoch in share_times:
                if epoch in resource_times and resource_times[epoch] >= share_times[epoch]:
                    latency = round(
                        (resource_times[epoch] - share_times[epoch]) * 1000)  # Convert to integer milliseconds
                    valid_latencies.append(latency)
            # Compute average latency or return 'NULL'
            if valid_latencies:
                avg_latency = round(sum(valid_latencies) / len(valid_latencies))
                latencies.append((f'Primary-{idx}', avg_latency))
            else:
                latencies.append((f'Primary-{idx}', 'NULL'))
        return latencies

    def _breeze_gather_latency_per_primary(self):
        """Calculate average gather latency for each primary in milliseconds"""
        latencies = []
        for idx, (shares, gathers) in enumerate(zip(self.shares_per_primary, self.gathers_per_primary)):
            if not shares or not gathers:
                latencies.append((f'Primary-{idx}', 'NULL'))
                continue
            # Build epoch-to-timestamp mappings
            share_times = {s['epoch']: s['timestamp'] for s in shares}
            gather_times = {r['epoch']: r['timestamp'] for r in gathers}
            # Calculate valid latency for matching epochs
            valid_latencies = []
            for epoch in share_times:
                if epoch in gather_times and gather_times[epoch] >= share_times[epoch]:
                    latency = round(
                        (gather_times[epoch] - share_times[epoch]) * 1000)  # Convert to integer milliseconds
                    valid_latencies.append(latency)
            # Compute average latency or return 'NULL'
            if valid_latencies:
                avg_latency = round(sum(valid_latencies) / len(valid_latencies))
                latencies.append((f'Primary-{idx}', avg_latency))
            else:
                latencies.append((f'Primary-{idx}', 'NULL'))
        return latencies

    def _beacon_errors(self):
        """calculate beacon equivocations in primaries"""
        beacon_values = {}
        for beacons in self.beacons_per_primary:
            for b in beacons:
                key = (b['epoch'], b['index'])
                if key not in beacon_values:
                    beacon_values[key] = set()
                beacon_values[key].add(b['value'])

        errors = 0
        for key, values in beacon_values.items():
            if len(values) > 1:
                errors += 1
        return errors

    def result(self):
        header_size = self.configs[0]['header_size']
        max_header_delay = self.configs[0]['max_header_delay']
        gc_depth = self.configs[0]['gc_depth']
        sync_retry_delay = self.configs[0]['sync_retry_delay']
        sync_retry_nodes = self.configs[0]['sync_retry_nodes']
        batch_size = self.configs[0]['batch_size']
        max_batch_delay = self.configs[0]['max_batch_delay']
        dag_waves_per_epoch = self.configs[0]['dag_waves_per_epoch']
        beacons_per_epoch = self.configs[0]['beacons_per_epoch']

        consensus_latency = self._consensus_latency() * 1_000
        consensus_tps, consensus_bps, _ = self._consensus_throughput()
        end_to_end_tps, end_to_end_bps, duration = self._end_to_end_throughput()
        end_to_end_latency = self._end_to_end_latency() * 1_000
        # add beacon
        beacon_rates = self._beacon_rate_per_primary()
        beacon_resource_rates = self._beacon_resource_rate_per_primary()
        beacon_latencies = self._beacon_latency_per_primary()
        gather_latencies = self._breeze_gather_latency_per_primary()
        beacon_errors = self._beacon_errors()

        output = (
            '\n'
            '-----------------------------------------\n'
            ' SUMMARY:\n'
            '-----------------------------------------\n'
            ' + CONFIG:\n'
            f' Faults: {self.faults} node(s)\n'
            f' Committee size: {self.committee_size} node(s)\n'
            f' Worker(s) per node: {self.workers} worker(s)\n'
            f' Collocate primary and workers: {self.collocate}\n'
            f' Input rate: {sum(self.rate):,} tx/s\n'
            f' Transaction size: {self.size[0]:,} B\n'
            f' Execution time: {round(duration):,} s\n'
            '\n'
            f' Header size: {header_size:,} B\n'
            f' Max header delay: {max_header_delay:,} ms\n'
            f' GC depth: {gc_depth:,} round(s)\n'
            f' Sync retry delay: {sync_retry_delay:,} ms\n'
            f' Sync retry nodes: {sync_retry_nodes:,} node(s)\n'
            f' batch size: {batch_size:,} B\n'
            f' Max batch delay: {max_batch_delay:,} ms\n'
            f' DAG leaders per epoch: {dag_waves_per_epoch:,}\n'
            f' Max beacon requests per epoch: {beacons_per_epoch:,}\n'
            '\n'
            # ' + RESULTS:\n'
            # f' Consensus TPS: {round(consensus_tps):,} tx/s\n'
            # f' Consensus BPS: {round(consensus_bps):,} B/s\n'
            # f' Consensus latency: {round(consensus_latency):,} ms\n'
            # '\n'
            # f' End-to-end TPS: {round(end_to_end_tps):,} tx/s\n'
            # f' End-to-end BPS: {round(end_to_end_bps):,} B/s\n'
            # f' End-to-end latency: {round(end_to_end_latency):,} ms\n'
            # '\n'
        )
        if beacons_per_epoch == 0:
            output += (
                ' + CONSENSUS RESULTS:\n'
                f' Consensus TPS: {round(consensus_tps):,} tx/s\n'
                f' Consensus BPS: {round(consensus_bps):,} B/s\n'
                f' Consensus latency: {round(consensus_latency):,} ms\n'
                '\n'
                f' End-to-end TPS: {round(end_to_end_tps):,} tx/s\n'
                f' End-to-end BPS: {round(end_to_end_bps):,} B/s\n'
                f' End-to-end latency: {round(end_to_end_latency):,} ms\n'
                '\n'
            )
        if beacons_per_epoch > 0:
            output += ' + BEACON RESULTS:\n'
            for (primary_name, beacon_rate), (_, resource_rate), (_, gather_latency), (_, latency) in zip(beacon_rates, beacon_resource_rates, gather_latencies,
                                                                                     beacon_latencies):
                latency_str = f'{latency:,}' if isinstance(latency, (int, float)) else latency
                gather_latency_str = f'{gather_latency:,}' if isinstance(gather_latency, (int, float)) else gather_latency

                output += f' {primary_name} Beacon Output Rate: {beacon_rate:,.3f} beacons/s, Beacon Resource Generation Rate: {resource_rate:,.3f} beacons/s, Gather Latency: {gather_latency_str} ms, Beacon Latency: {latency_str} ms\n'
            output += f' Beacon Equivocation Errors: {beacon_errors:,}\n'
            output += '-----------------------------------------\n'

        return output

    def print(self, filename):
        assert isinstance(filename, str)
        with open(filename, 'a') as f:
            f.write(self.result())

    @classmethod
    def process(cls, directory, faults=0):
        assert isinstance(directory, str)

        clients = []
        for filename in sorted(glob(join(directory, 'client-*.log'))):
            with open(filename, 'r') as f:
                clients += [f.read()]
        primaries = []
        for filename in sorted(glob(join(directory, 'primary-*.log'))):
            with open(filename, 'r') as f:
                primaries += [f.read()]
        workers = []
        for filename in sorted(glob(join(directory, 'worker-*.log'))):
            with open(filename, 'r') as f:
                workers += [f.read()]

        return cls(clients, primaries, workers, faults=faults)