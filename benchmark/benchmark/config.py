# Copyright(C) Facebook, Inc. and its affiliates.
from json import dump, load
from collections import OrderedDict
import json

class ConfigError(Exception):
    pass


class Key:
    def __init__(self, name, secret):
        self.name = name
        self.secret = secret

    @classmethod
    def from_file(cls, filename):
        assert isinstance(filename, str)
        with open(filename, 'r') as f:
            data = load(f)
        return cls(data['name'], data['secret'])


class Committee:
    ''' The committee looks as follows:
        "authorities: {
            "name": {
                "stake": 1,
                "primary: {
                    "primary_to_primary": x.x.x.x:x,
                    "worker_to_primary": x.x.x.x:x,
                },
                "workers": {
                    "0": {
                        "primary_to_worker": x.x.x.x:x,
                        "worker_to_worker": x.x.x.x:x,
                        "transactions": x.x.x.x:x
                    },
                    ...
                }
            },
            ...
        }
    '''

    def __init__(self, addresses, base_port):
        ''' The `addresses` field looks as follows:
            {
                "name": ["host", "host", ...],
                ...
            }
        '''
        assert isinstance(addresses, OrderedDict)
        assert all(isinstance(x, str) for x in addresses.keys())
        assert all(
            isinstance(x, list) and len(x) > 1 for x in addresses.values()
        )
        assert all(
            isinstance(x, str) for y in addresses.values() for x in y
        )
        assert len({len(x) for x in addresses.values()}) == 1
        assert isinstance(base_port, int) and base_port > 1024

        port = base_port
        self.json = {'authorities': OrderedDict()}
        for name, hosts in addresses.items():
            host = hosts.pop(0)
            primary_addr = {
                'primary_to_primary': f'{host}:{port}',
                'worker_to_primary': f'{host}:{port + 1}',
                'breeze_addr': f'{host}:{port+2}',
                'init_bft_addr': f'{host}:{port + 3}',
            }
            port += 4

            workers_addr = OrderedDict()
            for j, host in enumerate(hosts):
                workers_addr[j] = {
                    'primary_to_worker': f'{host}:{port}',
                    'transactions': f'{host}:{port + 1}',
                    'worker_to_worker': f'{host}:{port + 2}',
                }
                port += 3

            self.json['authorities'][name] = {
                'stake': 1,
                'primary': primary_addr,
                'workers': workers_addr
            }

    def primary_addresses(self, faults=0):
        ''' Returns an ordered list of primaries' addresses. '''
        assert faults < self.size()
        addresses = []
        good_nodes = self.size() - faults
        for authority in list(self.json['authorities'].values())[:good_nodes]:
            addresses += [authority['primary']['primary_to_primary']]
        return addresses

    def workers_addresses(self, faults=0):
        ''' Returns an ordered list of list of workers' addresses. '''
        assert faults < self.size()
        addresses = []
        good_nodes = self.size() - faults
        for authority in list(self.json['authorities'].values())[:good_nodes]:
            authority_addresses = []
            for id, worker in authority['workers'].items():
                authority_addresses += [(id, worker['transactions'])]
            addresses.append(authority_addresses)
        return addresses

    def ips(self, name=None):
        ''' Returns all the ips associated with an authority (in any order). '''
        if name is None:
            names = list(self.json['authorities'].keys())
        else:
            names = [name]

        ips = set()
        for name in names:
            addresses = self.json['authorities'][name]['primary']
            ips.add(self.ip(addresses['primary_to_primary']))
            ips.add(self.ip(addresses['worker_to_primary']))

            for worker in self.json['authorities'][name]['workers'].values():
                ips.add(self.ip(worker['primary_to_worker']))
                ips.add(self.ip(worker['worker_to_worker']))
                ips.add(self.ip(worker['transactions']))

        return list(ips)

    def remove_nodes(self, nodes):
        ''' remove the `nodes` last nodes from the committee. '''
        assert nodes < self.size()
        for _ in range(nodes):
            self.json['authorities'].popitem()

    def size(self):
        ''' Returns the number of authorities. '''
        return len(self.json['authorities'])

    def workers(self):
        ''' Returns the total number of workers (all authorities altogether). '''
        return sum(len(x['workers']) for x in self.json['authorities'].values())

    def print(self, filename):
        assert isinstance(filename, str)
        with open(filename, 'w') as f:
            dump(self.json, f, indent=4, sort_keys=True)

    @staticmethod
    def ip(address):
        assert isinstance(address, str)
        return address.split(':')[0]

class LocalCommittee(Committee):
    def __init__(self, names, port, workers):
        assert isinstance(names, list)
        assert all(isinstance(x, str) for x in names)
        assert isinstance(port, int)
        assert isinstance(workers, int) and workers > 0
        addresses = OrderedDict((x, ['127.0.0.1']*(1+workers)) for x in names)
        super().__init__(addresses, port)


class NodeParameters:
    def __init__(self, json):
        inputs = []
        try:
            inputs += [json['header_size']]
            inputs += [json['max_header_delay']]
            inputs += [json['gc_depth']]
            inputs += [json['sync_retry_delay']]
            inputs += [json['sync_retry_nodes']]
            inputs += [json['batch_size']]
            inputs += [json['max_batch_delay']]
            inputs += [json['beacon_req_delay']]
            inputs += [json['breeze_epoch_limit']]
            inputs += [json['eval_beacon']]
            if 'timeout' in json:
                inputs += [json['timeout']]

        except KeyError as e:
            raise ConfigError(f'Malformed parameters: missing key {e}')

        if not all(isinstance(x, int) for x in inputs):
            raise ConfigError('Invalid parameters type')

        self.json = json

    def print(self, filename):
        assert isinstance(filename, str)
        with open(filename, 'w') as f:
            dump(self.json, f, indent=4, sort_keys=True)


class BenchParameters:
    def __init__(self, json):
        try:
            self.faults = int(json['faults'])

            nodes = json['nodes']
            nodes = nodes if isinstance(nodes, list) else [nodes]
            if not nodes or any(x <= 1 for x in nodes):
                raise ConfigError('Missing or invalid number of nodes')
            self.nodes = [int(x) for x in nodes]

            rate = json['rate']
            rate = rate if isinstance(rate, list) else [rate]
            if not rate:
                raise ConfigError('Missing input rate')
            self.rate = [int(x) for x in rate]

            self.workers = int(json['workers'])

            if 'collocate' in json:
                self.collocate = bool(json['collocate'])
            else:
                self.collocate = True

            self.tx_size = int(json['tx_size'])

            self.duration = int(json['duration'])

            self.runs = int(json['runs']) if 'runs' in json else 1

            if 'protocol' not in json:
                self.protocol = 'tusk'
            elif json['protocol'] == 'tusk' or json['protocol'] == 'dolphin':
                self.protocol = json['protocol']
            else:
                protocol = json['protocol']
                raise ConfigError(f'Unsupported protocol "{protocol}"')

            if 'crypto' not in json:
                self.crypto = 'origin'
            elif json['crypto'] == 'post_quantum':
                self.crypto = 'pq'
            elif json['crypto'] == 'origin':
                self.crypto = 'origin'
            else:
                crypto = json['crypto']
                raise ConfigError(f'Unsupported crypto "{crypto}"')

            self.leader_per_epoch = int(json['leader_per_epoch']) if 'leader_per_epoch' in json else 20
            if 'n' in json and self.leader_per_epoch <= 0:
                raise ConfigError('leader_per_epoch must be a positive integer')

            self.avss_batch_size = int(json['avss_batch_size']) if 'avss_batch_size' in json else 200
            if 'n' in json and self.leader_per_epoch <= 0:
                raise ConfigError('avss_batch_size must be a positive integer')

            # New fields: n, log_q, g, kappa, r, ell with default 0
            self.n = int(json['n']) if 'n' in json else 128
            if 'n' in json and self.n <= 0:
                raise ConfigError('n must be a positive integer')

            self.log_q = int(json['log_q']) if 'log_q' in json else 32
            if 'log_q' in json and self.log_q <= 0:
                raise ConfigError('log_q must be a positive integer')

            self.g = int(json['g']) if 'g' in json else 1
            if 'g' in json and self.g <= 0:
                raise ConfigError('g must be a positive integer')

            self.kappa = int(json['kappa']) if 'kappa' in json else 128
            if 'kappa' in json and self.kappa <= 0:
                raise ConfigError('kappa must be a positive integer')

            self.r = int(json['r']) if 'r' in json else 2
            if 'r' in json and self.r <= 0:
                raise ConfigError('r must be a positive integer')

            self.ell = int(json['ell']) if 'ell' in json else 0
            if 'ell' in json and self.ell < 0:
                raise ConfigError('ell must be zero (case no folding) or a positive integer')

            if self.avss_batch_size < self.leader_per_epoch:
                raise ConfigError('avss_batch_size must be bigger than or equal with leader_per_epoch')

            if self.crypto == 'pq' and (self.n * self.kappa) / self.g < self.avss_batch_size:
                raise ConfigError('a batch of secrets:(n * kappa) / g must be bigger than or equal with avss_batch_size(e.g. a batch of randomness)')

            t = (min(self.nodes) - 1) // 3
            if self.crypto == 'pq' and self.r ** (self.ell + 1) < t + 1:
                raise ConfigError('r^(ell+1) must be bigger than or equal with t+1, since t+1 is the number of coefficients of polynomials')

        except KeyError as e:
            raise ConfigError(f'Malformed bench parameters: missing key {e}')

        except ValueError:
            raise ConfigError('Invalid parameters type')

        if min(self.nodes) <= self.faults:
            raise ConfigError('There should be more nodes than faults')

class PlotParameters:
    def __init__(self, params):
        assert isinstance(params, dict)
        for key in ['faults', 'nodes', 'workers', 'collocate', 'tx_size', 'protocol', 'crypto', 'rate', 'eval_beacon']:
            if key not in params:
                raise ConfigError(f'Missing {key} parameter')

        self.faults = params['faults']
        self.nodes = params['nodes']
        self.workers = params['workers']
        self.collocate = params['collocate']
        self.tx_size = params['tx_size']
        self.protocol = params['protocol']
        self.crypto = params['crypto']
        self.rate = params['rate']
        self.eval_beacon = params['eval_beacon']


    def scalability(self):
        return len(self.workers) > 1