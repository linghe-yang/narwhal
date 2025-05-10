# Copyright(C) Facebook, Inc. and its affiliates.
from re import search
from collections import defaultdict
from statistics import mean, stdev, median
from glob import glob
from copy import deepcopy
from os.path import join
import os

from benchmark.utils import PathMaker


class Setup:
    def __init__(self, faults, nodes, workers, collocate, rate, tx_size, protocol, crypto, test_beacon, beacon_batch_size=None):
        self.nodes = nodes
        self.workers = workers
        self.collocate = collocate
        self.rate = rate
        self.tx_size = tx_size
        self.faults = faults
        self.protocol = protocol
        self.crypto = crypto
        self.test_beacon = test_beacon
        self.beacon_batch_size = beacon_batch_size

    def __str__(self):
        result = (
            f' Faults: {self.faults}\n'
            f' Committee size: {self.nodes}\n'
            f' Workers per node: {self.workers}\n'
            f' Collocate primary and workers: {self.collocate}\n'
            f' Input rate: {self.rate} tx/s\n'
            f' Transaction size: {self.tx_size} B\n'
            f' Protocol: {self.protocol}\n'
            f' Crypto: {self.crypto}\n'
            f' Test Beacon: {self.test_beacon}\n'
        )
        if self.test_beacon and self.beacon_batch_size is not None:
            result += f' Beacon batch size: {self.beacon_batch_size}\n'
        return result

    def __eq__(self, other):
        return isinstance(other, Setup) and str(self) == str(other)

    def __hash__(self):
        return hash(str(self))

    @classmethod
    def from_str(cls, raw, protocol, crypto, test_beacon):
        faults = int(search(r'Faults: (\d+)', raw).group(1))
        nodes = int(search(r'Committee size: (\d+)', raw).group(1))
        workers = int(search(r'Worker\(s\) per node: (\d+)', raw).group(1))
        collocate = 'True' == search(
            r'Collocate primary and workers: (True|False)', raw
        ).group(1)
        rate = int(search(r'Input rate: (\d+)', raw).group(1))
        tx_size = int(search(r'Transaction size: (\d+)', raw).group(1))
        beacon_batch_size = None
        if test_beacon:
            beacon_batch_size = int(search(r'Max beacon requests per epoch: (\d+)', raw).group(1))
        return cls(faults, nodes, workers, collocate, rate, tx_size, protocol, crypto, test_beacon, beacon_batch_size)


class Result:
    def __init__(self, mean_tps=None, mean_latency=None, std_tps=0, std_latency=0,
                 mean_beacon_output=None, std_beacon_output=0,
                 mean_beacon_resource=None, std_beacon_resource=0,
                 mean_gather_latency=None, std_gather_latency=0,
                 mean_beacon_latency=None, std_beacon_latency=0):
        self.mean_tps = mean_tps
        self.mean_latency = mean_latency
        self.std_tps = std_tps
        self.std_latency = std_latency
        self.mean_beacon_output = mean_beacon_output
        self.std_beacon_output = std_beacon_output
        self.mean_beacon_resource = mean_beacon_resource
        self.std_beacon_resource = std_beacon_resource
        self.mean_gather_latency = mean_gather_latency
        self.std_gather_latency = std_gather_latency
        self.mean_beacon_latency = mean_beacon_latency
        self.std_beacon_latency = std_beacon_latency

    def __str__(self):
        result = ''
        if self.mean_tps is not None:
            result += f' TPS: {self.mean_tps} +/- {self.std_tps} tx/s\n'
            result += f' Latency: {self.mean_latency} +/- {self.std_latency} ms\n'
        if self.mean_beacon_output is not None:
            result += f' Beacon Output Rate: {self.mean_beacon_output:.3f} +/- {self.std_beacon_output:.3f} beacons/s\n'
            result += f' Beacon Resource Generation Rate: {self.mean_beacon_resource:.3f} +/- {self.std_beacon_resource:.3f} beacons/s\n'
            result += f' Gather Latency: {self.mean_gather_latency} +/- {self.std_gather_latency} ms\n'
            result += f' Beacon Latency: {self.mean_beacon_latency} +/- {self.std_beacon_latency} ms\n'
        return result

    @classmethod
    def from_str(cls, raw, test_beacon=False):
        if not test_beacon:
            tps = int(search(r'End-to-end TPS: (\d+)', raw).group(1))
            latency = int(search(r'End-to-end latency: (\d+)', raw).group(1))
            return cls(mean_tps=tps, mean_latency=latency)
        else:
            # Parse Beacon Results
            beacon_lines = [line for line in raw.split('\n') if 'Primary-' in line]
            output_rates = []
            resource_rates = []
            gather_latencies = []
            beacon_latencies = []
            for line in beacon_lines:
                output = float(search(r'Beacon Output Rate: (\d+\.\d+)', line).group(1))
                resource = float(search(r'Beacon Resource Generation Rate: (\d+\.\d+)', line).group(1))
                gather = int(search(r'Gather Latency: (\d+)', line).group(1))
                beacon = int(search(r'Beacon Latency: (\d+)', line).group(1))
                output_rates.append(output)
                resource_rates.append(resource)
                gather_latencies.append(gather)
                beacon_latencies.append(beacon)

            # Filter outliers in Beacon Output Rate
            median_output = median(output_rates)
            threshold = 0.2 * median_output  # Allow 20% deviation
            valid_indices = [i for i, x in enumerate(output_rates) if abs(x - median_output) <= threshold]

            if not valid_indices:
                return cls()  # Return empty result if no valid data

            valid_outputs = [output_rates[i] for i in valid_indices]
            valid_resources = [resource_rates[i] for i in valid_indices]
            valid_gathers = [gather_latencies[i] for i in valid_indices]
            valid_beacons = [beacon_latencies[i] for i in valid_indices]

            return cls(
                mean_beacon_output=mean(valid_outputs),
                std_beacon_output=stdev(valid_outputs) if len(valid_outputs) > 1 else 0,
                mean_beacon_resource=mean(valid_resources),
                std_beacon_resource=stdev(valid_resources) if len(valid_resources) > 1 else 0,
                mean_gather_latency=mean(valid_gathers),
                std_gather_latency=stdev(valid_gathers) if len(valid_gathers) > 1 else 0,
                mean_beacon_latency=mean(valid_beacons),
                std_beacon_latency=stdev(valid_beacons) if len(valid_beacons) > 1 else 0
            )

    @classmethod
    def aggregate(cls, results):
        if len(results) == 1:
            return results[0]

        result_dict = {}
        if results[0].mean_tps is not None:
            mean_tps = round(mean([x.mean_tps for x in results]))
            mean_latency = round(mean([x.mean_latency for x in results]))
            std_tps = round(stdev([x.mean_tps for x in results])) if len(results) > 1 else 0
            std_latency = round(stdev([x.mean_latency for x in results])) if len(results) > 1 else 0
            result_dict.update({
                'mean_tps': mean_tps,
                'std_tps': std_tps,
                'mean_latency': mean_latency,
                'std_latency': std_latency
            })
        if results[0].mean_beacon_output is not None:
            mean_beacon_output = mean([x.mean_beacon_output for x in results])
            std_beacon_output = stdev([x.mean_beacon_output for x in results]) if len(results) > 1 else 0
            mean_beacon_resource = mean([x.mean_beacon_resource for x in results])
            std_beacon_resource = stdev([x.mean_beacon_resource for x in results]) if len(results) > 1 else 0
            mean_gather_latency = mean([x.mean_gather_latency for x in results])
            std_gather_latency = stdev([x.mean_gather_latency for x in results]) if len(results) > 1 else 0
            mean_beacon_latency = mean([x.mean_beacon_latency for x in results])
            std_beacon_latency = stdev([x.mean_beacon_latency for x in results]) if len(results) > 1 else 0
            result_dict.update({
                'mean_beacon_output': mean_beacon_output,
                'std_beacon_output': std_beacon_output,
                'mean_beacon_resource': mean_beacon_resource,
                'std_beacon_resource': std_beacon_resource,
                'mean_gather_latency': mean_gather_latency,
                'std_gather_latency': std_gather_latency,
                'mean_beacon_latency': mean_beacon_latency,
                'std_beacon_latency': std_beacon_latency
            })
        return cls(**result_dict)


class LogAggregator:
    def __init__(self, nodes, protocols, cryptos, rate, eval_beacon):
        assert isinstance(nodes, list)
        assert all(isinstance(x, int) for x in nodes)
        assert isinstance(protocols, list)
        assert isinstance(cryptos, list)
        assert isinstance(rate, int)
        assert isinstance(eval_beacon, bool)

        self.nodes = nodes
        self.protocols = protocols
        self.cryptos = cryptos
        self.rate = rate
        self.eval_beacon = eval_beacon

        records = defaultdict(list)
        for protocol in protocols:
            for crypto in cryptos:
                test_beacons = [False, True] if self.eval_beacon else [False]
                for test_beacon in test_beacons:
                    pattern = PathMaker.result_file(0, '*', 1, True, self.rate, 512, protocol, crypto, test_beacon)
                    for filename in glob(pattern):
                        with open(filename, 'r') as f:
                            data = f.read()
                        for chunk in data.replace(',', '').split('SUMMARY')[1:]:
                            if chunk:
                                records[Setup.from_str(chunk, protocol, crypto, test_beacon)] += [
                                    Result.from_str(chunk, test_beacon)
                                ]

        self.records = {k: Result.aggregate(v) for k, v in records.items()}

    def print(self):
        if not os.path.exists(PathMaker.plots_path()):
            os.makedirs(PathMaker.plots_path())

        results = [self._print_tps()]
        if self.eval_beacon:
            results.append(self._print_beacon())

        for name, records in results:
            for setup, values in records.items():
                data = '\n'.join(
                    f' Variable value: X={x}\n{y}' for x, y in values
                )
                string = (
                    '\n'
                    '-----------------------------------------\n'
                    ' RESULTS:\n'
                    '-----------------------------------------\n'
                    f'{setup}'
                    '\n'
                    f'{data}'
                    '-----------------------------------------\n'
                )

                filename = PathMaker.agg_file(
                    name,
                    setup.faults,
                    setup.nodes if name != 'tps' else 'x',
                    setup.workers,
                    setup.collocate,
                    setup.rate,
                    setup.tx_size,
                    protocol=setup.protocol,
                    crypto=setup.crypto,
                    test_beacon=setup.test_beacon
                )
                with open(filename, 'w') as f:
                    f.write(string)

    def _print_tps(self):
        records = deepcopy(self.records)
        organized = defaultdict(list)
        for setup, result in records.items():
            if result.mean_tps is None or setup.test_beacon:
                continue
            setup = deepcopy(setup)
            variable = setup.nodes
            setup.nodes = 'x'
            organized[setup] += [(variable, result)]
        [v.sort(key=lambda x: x[0]) for v in organized.values()]
        return 'tps', organized

    def _print_beacon(self):
        records = deepcopy(self.records)
        organized = defaultdict(list)
        for setup, result in records.items():
            if result.mean_beacon_output is None or not setup.test_beacon:
                continue
            setup = deepcopy(setup)
            variable = setup.nodes
            setup.nodes = 'x'
            organized[setup] += [(variable, result)]
        [v.sort(key=lambda x: x[0]) for v in organized.values()]
        return 'beacon', organized