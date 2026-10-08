#!/usr/bin/env python3
"""Fail closed on incomplete, stale, mixed-architecture or corrupted release assets."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import subprocess
import tarfile
import tempfile

VERSION = '4.1.0'
WINDOWS = {
    'x64': (0x8664, 'x86_64-pc-windows-msvc', 'x64'),
    'x86': (0x014c, 'i686-pc-windows-msvc', 'x64'),
    'arm64': (0xaa64, 'aarch64-pc-windows-msvc', 'arm64'),
}
LINUX = {
    'x64': (62, 'x86_64-unknown-linux-gnu', 'x86_64'),
    'arm64': (183, 'aarch64-unknown-linux-gnu', 'aarch64'),
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def version(value):
    require(re.fullmatch(r'\d+\.\d+(?:\.\d+)?', value), 'Invalid glibc version')
    return tuple(map(int, value.split('.'))) + (0,) * (3 - len(value.split('.')))


def verify(directory, commit, check_sums=False):
    require(re.fullmatch(r'[0-9a-f]{40}', commit), 'Expected full source commit SHA')
    payloads = []
    partials = []
    for system, targets, extension in [('windows', WINDOWS, 'exe'), ('linux', LINUX, 'tar.gz')]:
        for arch, (machine, target, host) in targets.items():
            filename = f'nebulabook-v{VERSION}-{system}-{arch}.{extension}'
            info_name = f'build-info-{system}-{arch}.json'
            data = (directory / filename).read_bytes()
            info = json.loads((directory / info_name).read_text(encoding='utf-8-sig'))
            require(info['commit'] == commit and info['version'] == VERSION, 'Stale source/version: ' + info_name)
            require(info['arch'] == arch and info['target'] == target and info['host_arch'].lower() == host,
                    'Target/host mismatch: ' + info_name)
            require(info['bytes'] == len(data) and info['sha256'] == sha(data), 'Payload checksum/size mismatch: ' + filename)
            require(isinstance(info['rust'], str) and info['rust'].startswith('rustc '), 'Missing compiler provenance')
            partial_name = f'SHA256SUMS-{system}-{arch}.txt'
            partial_text = (directory / partial_name).read_text(encoding='ascii')
            expected_partial = f'{sha(data)}  {filename}\n'
            if system == 'windows':
                require(len(data) > 128 and data[:2] == b'MZ', 'Not a Windows EXE')
                offset = struct.unpack_from('<I', data, 0x3c)[0]
                require(offset + 26 <= len(data) and data[offset:offset + 4] == b'PE\0\0', 'Invalid PE header')
                require(struct.unpack_from('<H', data, offset + 4)[0] == machine and info['pe_machine'] == f'{machine:04x}', 'Wrong PE machine')
            else:
                require(info['release_eligible'] is True and info['native_execution_verified'] is True,
                        'Linux artifact is local-only or has no real display validation')
                require(info['host_os'] == 'Linux' and info['file'] == filename and info['elf_class'] == 64 and info['elf_machine'] == machine,
                        'Invalid Linux metadata')
                require(version(info['glibc_minimum']) <= version(info['glibc_baseline']) <= version('2.35'), 'Linux glibc baseline exceeded')
                prefix = filename[:-7]
                names = {'nebulabook', 'README.md', 'LICENSE', 'nebulabook.desktop', 'install-linux.sh', 'SHA256SUMS'}
                with tarfile.open(directory / filename, 'r:gz') as bundle:
                    members = bundle.getmembers()
                    require(len(members) == len(names) + 1, 'Unexpected tar members')
                    require(len({member.name for member in members}) == len(members), 'Duplicate tar members')
                    require({member.name.rstrip('/') for member in members} == {prefix, *(f'{prefix}/{name}' for name in names)}, 'Unsafe/incomplete tar paths')
                    files = {}
                    for member in members:
                        if member.name.rstrip('/') == prefix:
                            require(member.isdir(), 'Package root is not directory')
                        else:
                            require(member.isfile() and 0 <= member.size <= 64 * 1024 * 1024, 'Unsafe tar entry')
                            files[member.name.split('/')[-1]] = bundle.extractfile(member).read()
                    binary = files['nebulabook']
                    require(len(binary) >= 64 and binary[:7] == b'\x7fELF\x02\x01\x01', 'Not ELF64 little-endian')
                    require(struct.unpack_from('<H', binary, 18)[0] == machine, 'Wrong ELF machine')
                    require(bundle.getmember(prefix + '/nebulabook').mode & 0o777 == 0o755, 'ELF is not executable')
                    require(bundle.getmember(prefix + '/install-linux.sh').mode & 0o777 == 0o755, 'Installer is not executable')
                    require(info['binary']['file'] == 'nebulabook' and info['binary']['bytes'] == len(binary) and info['binary']['sha256'] == sha(binary), 'Inner ELF checksum mismatch')
                    evidence = info.get('smoke_evidence')
                    require(isinstance(evidence, dict) and set(evidence) == {'x11', 'wayland'}, 'Missing Linux display evidence')
                    required_base = {'renderer-initialized', 'cjk-glyphs-present', 'process-alive'}
                    required_x11 = {'new-edit-save', 'normal-close', 'reopen-preserves-data', 'save-conflict-blocks-close', 'external-data-not-overwritten'}
                    for backend in ('x11', 'wayland'):
                        check = evidence[backend]
                        require(isinstance(check, dict) and check.get('binary_sha256') == sha(binary), 'Display evidence belongs to another binary')
                        require(check.get('file') == f'{backend}-result.json' and re.fullmatch(r'[0-9a-f]{64}', check.get('sha256', '')), 'Invalid display evidence provenance')
                        required = required_base | (required_x11 if backend == 'x11' else set())
                        checks = check.get('checks')
                        require(isinstance(checks, list) and all(isinstance(item, str) for item in checks) and required.issubset(checks), 'Incomplete display evidence checks')
                    internal = ''.join(f'{sha(files[name])}  {name}\n' for name in sorted(names - {'SHA256SUMS'}))
                    require(files['SHA256SUMS'].decode('ascii') == internal, 'Internal manifest mismatch')
                # Architecture-independent ABI inspection of the actual packaged ELF.
                with tempfile.TemporaryDirectory(prefix='nebulabook-verify-') as temporary:
                    elf = Path(temporary) / 'nebulabook'
                    elf.write_bytes(binary)
                    requirements = subprocess.check_output(['readelf', '--version-info', '--wide', str(elf)], text=True)
                versions = set(re.findall(r'Name:\s+GLIBC_(\d+\.\d+(?:\.\d+)?)', requirements))
                require(versions and max(versions, key=version) == info['glibc_minimum'], 'Actual ELF GLIBC requirements differ')
                expected_partial += f'{sha((directory / info_name).read_bytes())}  {info_name}\n'
            require(partial_text == expected_partial, 'Per-platform checksum manifest mismatch: ' + partial_name)
            payloads.extend([filename, info_name])
            partials.append(partial_name)
    expected_files = set(payloads + partials + (['SHA256SUMS.txt'] if (directory / 'SHA256SUMS.txt').exists() else []))
    require({path.name for path in directory.iterdir()} == expected_files, 'Unexpected/missing release files')
    combined = ''.join(f'{sha((directory / name).read_bytes())}  {name}\n' for name in sorted(payloads))
    if check_sums:
        require((directory / 'SHA256SUMS.txt').read_text(encoding='ascii') == combined, 'Combined checksum manifest mismatch')
    else:
        (directory / 'SHA256SUMS.txt').write_text(combined, encoding='ascii')
    print('Verified Linux x64/ARM64 and Windows x64/x86/ARM64 for source ' + commit)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('directory', type=Path)
    parser.add_argument('commit')
    parser.add_argument('--check-sums', action='store_true')
    args = parser.parse_args()
    verify(args.directory, args.commit, args.check_sums)
