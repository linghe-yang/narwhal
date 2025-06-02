# Copyright(C) Facebook, Inc. and its affiliates.
from collections import defaultdict
from copy import deepcopy
from os.path import join
from re import findall, search, split
import matplotlib.pyplot as plt
import matplotlib.ticker as tick
from glob import glob
from itertools import cycle

from benchmark.utils import PathMaker
from benchmark.config import PlotParameters
from benchmark.aggregate import LogAggregator

from matplotlib.ticker import LogLocator, LogFormatter


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
        linestyles = cycle(['--', ':', '-.'])
        self.results.sort(key=self._natural_keys, reverse=(type == 'tps'))
        for result in self.results:
            y_values, y_err = y_axis(result)
            x_values = self._variable(result)
            if len(y_values) != len(y_err) or len(y_err) != len(x_values):
                raise PlotError('Unequal number of x, y, and y_err values')

            plt.errorbar(
                x_values, y_values, yerr=y_err, label=z_axis(result),
                linestyle=next(linestyles), marker=next(markers), capsize=3
            )

        plt.legend(loc='lower center', bbox_to_anchor=(0.5, 1), ncol=2)

        plt.xlim(xmin=0)
        plt.ylim(bottom=0)
        if type == 'beacon_latency' or type == 'latency':
            plt.legend(loc='upper left', framealpha=0.5, frameon=True, fontsize=10, ncol=1)
        elif type == 'tps':
            plt.legend(loc='upper right', framealpha=0.5, frameon=True, fontsize=10, ncol=1)
            plt.ylim(bottom=50000)

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
            plt.savefig(PathMaker.plot_file(type, x), bbox_inches='tight', dpi=300)
    def _plot_beacon_rate(self, x_label, y_label, y_axis, z_axis, type):
        plt.figure(figsize=(10, 6))
        markers = cycle(['o', 'v', 's', 'p', 'D', 'P'])
        linestyles = cycle(['--', ':', '-.'])
        self.results.sort(key=self._natural_keys, reverse=(type == 'tps'))
        for result in self.results:
            y_values, y_err = y_axis(result)
            x_values = self._variable(result)
            if len(y_values) != len(y_err) or len(y_err) != len(x_values):
                raise PlotError('Unequal number of x, y, and y_err values')

            plt.errorbar(
                x_values, y_values, yerr=y_err, label=z_axis(result),
                linestyle=next(linestyles), marker=next(markers), capsize=3
            )
        plt.legend(loc='upper right', framealpha=0.5, frameon=True, fontsize=13, ncol=1)
        plt.xlim(xmin=0)
        plt.ylim(bottom=10, top=10000)
        plt.yscale('log')

        ax = plt.gca()
        ax.yaxis.set_major_locator(LogLocator(base=10.0))
        ax.yaxis.set_minor_locator(
            LogLocator(base=10.0, subs=(2, 3, 4, 5, 6, 7, 8, 9)))
        ax.yaxis.set_major_formatter(LogFormatter())
        ax.yaxis.set_minor_formatter(LogFormatter())
        ax.tick_params(axis='y', which='major', labelsize=10)
        ax.tick_params(axis='y', which='minor', labelsize=8)

        plt.xlabel(x_label, fontweight='bold')
        plt.ylabel(y_label[0], fontweight='bold')
        plt.xticks(weight='bold')
        plt.yticks(weight='bold')
        plt.grid(which='both', linestyle='--', alpha=0.5)

        ax.xaxis.set_major_formatter(default_major_formatter)
        if type in ['latency', 'gather_latency', 'beacon_latency']:
            ax.yaxis.set_major_formatter(sec_major_formatter)
        if len(y_label) > 1:
            secaxy = ax.secondary_yaxis(
                'right', functions=(self._tps2bps, self._bps2tps)
            )
            secaxy.set_ylabel(y_label[1])
            secaxy.set_yscale('log')
            secaxy.yaxis.set_major_locator(LogLocator(base=10.0))
            secaxy.yaxis.set_minor_locator(LogLocator(base=10.0, subs=(2, 3, 4, 5, 6, 7, 8, 9)))
            secaxy.yaxis.set_major_formatter(mb_major_formatter)
            secaxy.yaxis.set_minor_formatter(LogFormatter())
            secaxy.tick_params(axis='y', which='major', labelsize=10)
            secaxy.tick_params(axis='y', which='minor', labelsize=8)

        for x in ['pdf', 'png']:
            plt.savefig(PathMaker.plot_file(type, x), bbox_inches='tight', dpi=300)

    @staticmethod
    def protocol_crypto(data):
        protocol_match = search(r'Protocol: (\w+)', data)
        crypto_match = search(r'Crypto: (\w+)', data)

        if protocol_match:
            protocol = protocol_match.group(1)
            if crypto_match:
                crypto = crypto_match.group(1)
                return f'{protocol}-{crypto}'
            return f'{protocol}'

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
        ploter._plot_beacon_rate(x_label, y_label, ploter._beacon_output_rate, cls.protocol_crypto, 'beacon_output_rate')

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

        beacon_rate_files = deepcopy(beacon_files)
        beacon_rate_files += glob(join(PathMaker.plots_path(), 'spurt.txt'))
        beacon_rate_files += glob(join(PathMaker.plots_path(), 'asyrand.txt'))
        # Plot graphs based on eval_beacon
        if params.eval_beacon:
            cls.plot_tps(tps_files)
            cls.plot_latency(tps_files)
            cls.plot_beacon_output_rate(beacon_rate_files)
            cls.plot_beacon_resource_rate(beacon_files)
            cls.plot_gather_latency(beacon_files)
            cls.plot_beacon_latency(beacon_files)
        else:
            cls.plot_tps(tps_files)
            cls.plot_latency(tps_files)