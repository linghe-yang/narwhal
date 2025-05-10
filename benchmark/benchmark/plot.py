# Copyright(C) Facebook, Inc. and its affiliates.
from collections import defaultdict
from re import findall, search, split
import matplotlib.pyplot as plt
import matplotlib.ticker as tick
from glob import glob
from itertools import cycle

from benchmark.utils import PathMaker
from benchmark.config import PlotParameters
from benchmark.aggregate import LogAggregator


@tick.FuncFormatter
def default_major_formatter(x, pos):
    if pos is None:
        return
    if x >= 1_000:
        return f'{x/1000:.0f}k'
    else:
        return f'{x:.0f}'


@tick.FuncFormatter
def sec_major_formatter(x, pos):
    if pos is None:
        return
    return f'{int(x)}'


@tick.FuncFormatter
def mb_major_formatter(x, pos):
    if pos is None:
        return
    return f'{x:,.0f}'


class PlotError(Exception):
    pass


class Ploter:
    def __init__(self, filenames):
        if not filenames:
            raise PlotError('No data to plot')

        self.results = []
        try:
            for filename in filenames:
                with open(filename, 'r') as f:
                    self.results += [f.read().replace(',', '')]
        except OSError as e:
            raise PlotError(f'Failed to load log files: {e}')

    def _natural_keys(self, text):
        def try_cast(text): return int(text) if text.isdigit() else text
        return [try_cast(c) for c in split('(\d+)', text)]

    def _tps(self, data):
        values = findall(r' TPS: (\d+) \+/- (\d+)', data)
        values = [(int(x), int(y)) for x, y in values]
        return list(zip(*values))

    def _latency(self, data, scale=1):
        values = findall(r' Latency: (\d+) \+/- (\d+)', data)
        # Keep as ms, no conversion
        values = [(int(x) / 1000, int(y) / 1000) for x, y in values]
        return list(zip(*values))

    def _beacon_output_rate(self, data):
        values = findall(r' Beacon Output Rate: (\d+\.?\d*) \+/- (\d+\.?\d*)', data)
        # Convert from beacons/s to beacons/min (multiply by 60)
        values = [(float(x) * 60, float(y) * 60) for x, y in values]
        return list(zip(*values))

    def _beacon_resource_rate(self, data):
        values = findall(r' Beacon Resource Generation Rate: (\d+\.?\d*) \+/- (\d+\.?\d*)', data)
        # Convert from beacons/s to beacons/min (multiply by 60)
        values = [(float(x) * 60, float(y) * 60) for x, y in values]
        return list(zip(*values))

    def _beacon_batch_size(self, data):
        values = findall(r'Beacon batch size: (\d+)', data)
        if len(values) != 1:
            raise PlotError('Expected exactly one Beacon batch size value')
        return int(values[0])

    def _gather_latency(self, data):
        values = findall(r' Gather Latency: (\d+\.?\d*) \+/- (\d+\.?\d*)', data)
        batch_size = self._beacon_batch_size(data)
        # Convert to amortized latency (ms/beacon)
        values = [(int(float(x)  / batch_size), int(float(y)  / batch_size)) for x, y in values]
        return list(zip(*values))

    def _beacon_latency(self, data):
        values = findall(r' Beacon Latency: (\d+\.?\d*) \+/- (\d+\.?\d*)', data)
        batch_size = self._beacon_batch_size(data)
        # Convert to amortized latency (ms/beacon)
        values = [(int(float(x)  / batch_size), int(float(y)  / batch_size)) for x, y in values]
        return list(zip(*values))

    def _variable(self, data):
        return [int(x) for x in findall(r'Variable value: X=(\d+)', data)]

    def _tps2bps(self, x):
        data = self.results[0]
        size = int(search(r'Transaction size: (\d+)', data).group(1))
        return x * size / 10**6

    def _bps2tps(self, x):
        data = self.results[0]
        size = int(search(r'Transaction size: (\d+)', data).group(1))
        return x * 10**6 / size

    def _plot(self, x_label, y_label, y_axis, z_axis, type):
        plt.figure()
        markers = cycle(['o', 'v', 's', 'p', 'D', 'P'])
        self.results.sort(key=self._natural_keys, reverse=(type == 'tps'))
        for result in self.results:
            y_values, y_err = y_axis(result)
            x_values = self._variable(result)
            if len(y_values) != len(y_err) or len(y_err) != len(x_values):
                raise PlotError('Unequal number of x, y, and y_err values')

            plt.errorbar(
                x_values, y_values, yerr=y_err, label=z_axis(result),
                linestyle='dotted', marker=next(markers), capsize=3
            )

        plt.legend(loc='lower center', bbox_to_anchor=(0.5, 1), ncol=2)
        plt.xlim(xmin=0)
        plt.ylim(bottom=0)
        plt.xlabel(x_label, fontweight='bold')
        plt.ylabel(y_label[0], fontweight='bold')
        plt.xticks(weight='bold')
        plt.yticks(weight='bold')
        plt.grid()
        ax = plt.gca()
        ax.xaxis.set_major_formatter(default_major_formatter)
        ax.yaxis.set_major_formatter(default_major_formatter)
        if type in ['latency', 'gather_latency', 'beacon_latency']:
            ax.yaxis.set_major_formatter(sec_major_formatter)
        if len(y_label) > 1:
            secaxy = ax.secondary_yaxis(
                'right', functions=(self._tps2bps, self._bps2tps)
            )
            secaxy.set_ylabel(y_label[1])
            secaxy.yaxis.set_major_formatter(mb_major_formatter)

        for x in ['pdf', 'png']:
            plt.savefig(PathMaker.plot_file(type, x), bbox_inches='tight')

    @staticmethod
    def protocol_crypto(data):
        protocol = search(r'Protocol: (\w+)', data).group(1)
        crypto = search(r'Crypto: (\w+)', data).group(1)
        return f'{protocol}-{crypto}'

    @classmethod
    def plot_tps(cls, files):
        assert isinstance(files, list)
        assert all(isinstance(x, str) for x in files)
        x_label = 'Nodes'
        y_label = ['Throughput (tx/s)', 'Throughput (MB/s)']
        ploter = cls(files)
        ploter._plot(x_label, y_label, ploter._tps, cls.protocol_crypto, 'tps')

    @classmethod
    def plot_latency(cls, files):
        assert isinstance(files, list)
        assert all(isinstance(x, str) for x in files)
        x_label = 'Nodes'
        y_label = ['Latency (s)']
        ploter = cls(files)
        ploter._plot(x_label, y_label, ploter._latency, cls.protocol_crypto, 'latency')

    @classmethod
    def plot_beacon_output_rate(cls, files):
        assert isinstance(files, list)
        assert all(isinstance(x, str) for x in files)
        x_label = 'Nodes'
        y_label = ['Beacon Output Rate (beacons/min)']
        ploter = cls(files)
        ploter._plot(x_label, y_label, ploter._beacon_output_rate, cls.protocol_crypto, 'beacon_output_rate')

    @classmethod
    def plot_beacon_resource_rate(cls, files):
        assert isinstance(files, list)
        assert all(isinstance(x, str) for x in files)
        x_label = 'Nodes'
        y_label = ['Beacon Resource Generation Rate (beacons/min)']
        ploter = cls(files)
        ploter._plot(x_label, y_label, ploter._beacon_resource_rate, cls.protocol_crypto, 'beacon_resource_rate')

    @classmethod
    def plot_gather_latency(cls, files):
        assert isinstance(files, list)
        assert all(isinstance(x, str) for x in files)
        x_label = 'Nodes'
        y_label = ['Amortized Gather Latency (ms/beacon)']
        ploter = cls(files)
        ploter._plot(x_label, y_label, ploter._gather_latency, cls.protocol_crypto, 'gather_latency')

    @classmethod
    def plot_beacon_latency(cls, files):
        assert isinstance(files, list)
        assert all(isinstance(x, str) for x in files)
        x_label = 'Nodes'
        y_label = ['Amortized Beacon Latency (ms/beacon)']
        ploter = cls(files)
        ploter._plot(x_label, y_label, ploter._beacon_latency, cls.protocol_crypto, 'beacon_latency')

    @classmethod
    def plot(cls, params_dict):
        try:
            params = PlotParameters(params_dict)
        except PlotError as e:
            raise PlotError('Invalid nodes or bench parameters', e)

        # Aggregate the logs
        LogAggregator(params.nodes, params.protocol, params.crypto, params.rate, params.eval_beacon).print()

        # Collect files for plotting
        tps_files = []
        beacon_files = []
        for protocol in params.protocol:
            for crypto in params.crypto:
                tps_files += glob(PathMaker.agg_file('tps', 0, 'x', 1, True, params.rate, 512, protocol=protocol, crypto=crypto, test_beacon=False))
                if params.eval_beacon:
                    beacon_files += glob(PathMaker.agg_file('beacon', 0, 'x', 1, True, params.rate, 512, protocol=protocol, crypto=crypto, test_beacon=True))

        # Plot graphs based on eval_beacon
        if params.eval_beacon:
            cls.plot_tps(tps_files)
            cls.plot_latency(tps_files)
            cls.plot_beacon_output_rate(beacon_files)
            cls.plot_beacon_resource_rate(beacon_files)
            cls.plot_gather_latency(beacon_files)
            cls.plot_beacon_latency(beacon_files)
        else:
            cls.plot_tps(tps_files)
            cls.plot_latency(tps_files)