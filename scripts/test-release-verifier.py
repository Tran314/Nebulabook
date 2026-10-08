#!/usr/bin/env python3
"""Offline release-verifier contract tests; NEVER evidence of native/GUI execution.

Usage: python3 scripts/test-release-verifier.py [repository-root]
Requires Python 3.10+, cc, readelf and the repository's verify-release.py.
Creates/removes all test assets under TemporaryDirectory. Does not edit the repo.
PE headers, the other-architecture ELF header, and GUI evidence are SYNTHETIC.
"""
from pathlib import Path
import hashlib
import importlib.util
import io
import json
import struct
import subprocess
import sys
import tarfile
import tempfile

sys.dont_write_bytecode = True
root_arg = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('release_verify', root_arg / 'scripts/verify-release.py')
verify = importlib.util.module_from_spec(spec)
spec.loader.exec_module(verify)
COMMIT = '1' * 40


def sha(data):
    return hashlib.sha256(data).hexdigest()


with tempfile.TemporaryDirectory(prefix='nebulabook-review-release-') as temporary:
    root = Path(temporary)
    assets = root / 'assets'
    assets.mkdir()
    (root / 'main.c').write_text('int main(void) { return 0; }\n')
    subprocess.run(['cc', str(root / 'main.c'), '-o', str(root / 'base')], check=True)
    native = (root / 'base').read_bytes()
    requirements = subprocess.check_output(['readelf', '--version-info', '--wide', str(root / 'base')], text=True)
    glibc = max(verify.re.findall(r'Name:\s+GLIBC_(\d+\.\d+(?:\.\d+)?)', requirements), key=verify.version)

    def fix_partial(system, arch):
        extension = 'exe' if system == 'windows' else 'tar.gz'
        name = f'nebulabook-v4.1.0-{system}-{arch}.{extension}'
        meta = f'build-info-{system}-{arch}.json'
        body = f'{sha((assets / name).read_bytes())}  {name}\n'
        if system == 'linux':
            body += f'{sha((assets / meta).read_bytes())}  {meta}\n'
        (assets / f'SHA256SUMS-{system}-{arch}.txt').write_text(body)

    def emit(system, arch, data, extra):
        extension = 'exe' if system == 'windows' else 'tar.gz'
        name = f'nebulabook-v4.1.0-{system}-{arch}.{extension}'
        meta = f'build-info-{system}-{arch}.json'
        targets = verify.WINDOWS if system == 'windows' else verify.LINUX
        _, target, host = targets[arch]
        info = dict(commit=COMMIT, version='4.1.0', arch=arch, target=target,
                    host_arch=host, bytes=len(data), sha256=sha(data),
                    rust='rustc fixture-not-real-build', **extra)
        (assets / name).write_bytes(data)
        (assets / meta).write_text(json.dumps(info))
        fix_partial(system, arch)

    def tar_bytes(arch, binary, extra_path=None):
        prefix = f'nebulabook-v4.1.0-linux-{arch}'
        files = {'nebulabook': binary, 'README.md': b'SYNTHETIC TEST FIXTURE\n',
                 'LICENSE': b'test\n', 'nebulabook.desktop': b'test\n',
                 'install-linux.sh': b'#!/bin/sh\nexit 0\n'}
        files['SHA256SUMS'] = ''.join(f'{sha(files[name])}  {name}\n' for name in sorted(files)).encode()
        stream = io.BytesIO()
        with tarfile.open(fileobj=stream, mode='w:gz') as bundle:
            directory = tarfile.TarInfo(prefix)
            directory.type = tarfile.DIRTYPE
            directory.mode = 0o755
            bundle.addfile(directory)
            for name, content in files.items():
                item = tarfile.TarInfo(f'{prefix}/{name}')
                item.mode = 0o755 if name in ('nebulabook', 'install-linux.sh') else 0o644
                item.size = len(content)
                bundle.addfile(item, io.BytesIO(content))
            if extra_path:
                item = tarfile.TarInfo(extra_path)
                item.size = 1
                bundle.addfile(item, io.BytesIO(b'x'))
        return stream.getvalue()

    for arch, (machine, _, _) in verify.WINDOWS.items():
        pe = bytearray(256)
        pe[:2] = b'MZ'
        struct.pack_into('<I', pe, 0x3c, 128)
        pe[128:132] = b'PE\0\0'
        struct.pack_into('<H', pe, 132, machine)
        emit('windows', arch, bytes(pe), dict(pe_machine=f'{machine:04x}'))

    binaries = {}
    for arch, (machine, _, _) in verify.LINUX.items():
        binary = bytearray(native)
        struct.pack_into('<H', binary, 18, machine)
        binary = bytes(binary)
        binaries[arch] = binary
        evidence = {}
        for backend in ('x11', 'wayland'):
            checks = ['renderer-initialized', 'cjk-glyphs-present', 'process-alive']
            if backend == 'x11':
                checks += ['new-edit-save', 'normal-close', 'reopen-preserves-data',
                           'save-conflict-blocks-close', 'external-data-not-overwritten']
            evidence[backend] = dict(file=f'{backend}-result.json', sha256='2' * 64,
                                     binary_sha256=sha(binary), checks=checks)
        emit('linux', arch, tar_bytes(arch, binary),
             dict(file=f'nebulabook-v4.1.0-linux-{arch}.tar.gz', host_os='Linux',
                  elf_class=64, elf_machine=machine, release_eligible=True,
                  native_execution_verified=True, glibc_minimum=glibc,
                  glibc_baseline='2.35', binary=dict(file='nebulabook', bytes=len(binary),
                                                   sha256=sha(binary)), smoke_evidence=evidence))

    originals = {path.name: path.read_bytes() for path in assets.iterdir()}
    passed = []

    def restore():
        for path in assets.iterdir():
            path.unlink()
        for name, data in originals.items():
            (assets / name).write_bytes(data)

    def mutate_info(arch, action):
        path = assets / f'build-info-linux-{arch}.json'
        info = json.loads(path.read_text())
        action(info)
        path.write_text(json.dumps(info))
        fix_partial('linux', arch)

    def reject(label, mutate, expected):
        restore()
        mutate()
        try:
            verify.verify(assets, COMMIT)
        except (ValueError, KeyError, FileNotFoundError) as error:
            assert expected in str(error), (label, str(error), expected)
            passed.append(label)
            print('PASS rejected:', label, '=>', str(error))
        else:
            raise AssertionError('UNEXPECTED ACCEPT ' + label)

    verify.verify(assets, COMMIT)
    verify.verify(assets, COMMIT, True)
    passed += ['five-platform synthetic positive', 'combined manifest check']
    reject('stale source', lambda: mutate_info('x64', lambda info: info.update(commit='9' * 40)), 'Stale source/version')
    reject('missing GUI evidence', lambda: mutate_info('x64', lambda info: info.update(smoke_evidence={})), 'Missing Linux display evidence')
    reject('GUI evidence different binary', lambda: mutate_info('x64', lambda info: info['smoke_evidence']['x11'].update(binary_sha256='0' * 64)), 'Display evidence belongs to another binary')
    reject('GUI evidence incomplete checks', lambda: mutate_info('x64', lambda info: info['smoke_evidence']['wayland'].update(checks=[])), 'Incomplete display evidence checks')
    reject('GUI evidence malformed hash', lambda: mutate_info('x64', lambda info: info['smoke_evidence']['wayland'].update(sha256='not-a-sha')), 'Invalid display evidence provenance')
    reject('local-only metadata', lambda: mutate_info('x64', lambda info: info.update(release_eligible=False)), 'local-only')
    reject('glibc newer than 2.35', lambda: mutate_info('x64', lambda info: info.update(glibc_minimum='2.36', glibc_baseline='2.36')), 'Linux glibc baseline exceeded')
    reject('glibc declaration different from ELF', lambda: mutate_info('x64', lambda info: info.update(glibc_minimum='2.1')), 'Actual ELF GLIBC requirements differ')

    def alter_archive(arch, new_binary, extra_path=None):
        name = f'nebulabook-v4.1.0-linux-{arch}.tar.gz'
        data = tar_bytes(arch, new_binary, extra_path)
        (assets / name).write_bytes(data)

        def change(info):
            info.update(bytes=len(data), sha256=sha(data))
            info['binary'].update(bytes=len(new_binary), sha256=sha(new_binary))
            for evidence in info['smoke_evidence'].values():
                evidence['binary_sha256'] = sha(new_binary)

        mutate_info(arch, change)

    wrong = bytearray(binaries['x64'])
    struct.pack_into('<H', wrong, 18, 183)
    reject('ELF machine changed with recomputed hashes', lambda: alter_archive('x64', bytes(wrong)), 'Wrong ELF machine')
    reject('extra unsafe tar path', lambda: alter_archive('x64', binaries['x64'], '../../outside'), 'Unexpected tar members')

    def wrong_pe():
        name = 'nebulabook-v4.1.0-windows-x86.exe'
        binary = bytearray((assets / name).read_bytes())
        struct.pack_into('<H', binary, 132, 0x8664)
        (assets / name).write_bytes(binary)
        path = assets / 'build-info-windows-x86.json'
        info = json.loads(path.read_text())
        info.update(sha256=sha(binary))
        path.write_text(json.dumps(info))
        fix_partial('windows', 'x86')

    reject('PE machine changed with recomputed hash', wrong_pe, 'Wrong PE machine')
    reject('foreign asset mixed in', lambda: (assets / 'unexpected.txt').write_text('x'), 'Unexpected/missing release files')
    restore()
    verify.verify(assets, COMMIT)
    (assets / 'SHA256SUMS.txt').write_text('bad\n')
    try:
        verify.verify(assets, COMMIT, True)
    except ValueError as error:
        assert 'Combined checksum manifest mismatch' in str(error)
        passed.append('combined checksum tamper')
        print('PASS rejected: combined checksum tamper')
    else:
        raise AssertionError('UNEXPECTED ACCEPT combined checksum tamper')
    print(f'RESULT: {len(passed)} offline verifier cases passed. Synthetic PE/ARM headers and smoke metadata; no Windows, ARM64 or GUI execution occurred.')
