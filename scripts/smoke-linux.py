#!/usr/bin/env python3
"""Real display-server smoke checks; never substitutes for unit tests or real GPU QA."""
import json
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

binary, backend, output_arg = sys.argv[1:]
output = Path(output_arg)
processes = []
handles = []


def wait_for(condition, message, timeout=20):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if condition():
            return
        time.sleep(0.1)
    raise RuntimeError(message)


def launch(command, name, env):
    handle = (output / f'{backend}-{name}.log').open('w')
    handles.append(handle)
    process = subprocess.Popen(command, env=env, stdout=handle, stderr=subprocess.STDOUT)
    processes.append(process)
    return process


def read_log(name):
    return (output / f'{backend}-{name}.log').read_text(errors='replace')


def command(*args):
    return subprocess.check_output(args, text=True, timeout=15).strip()


try:
    with tempfile.TemporaryDirectory(prefix=f'nebulabook-{backend}-') as temporary:
        base = Path(temporary)
        env = os.environ.copy()
        for key, name in [('XDG_DATA_HOME', 'data'), ('XDG_CONFIG_HOME', 'config'),
                          ('XDG_CACHE_HOME', 'cache'), ('XDG_RUNTIME_DIR', 'runtime')]:
            path = base / name
            path.mkdir(mode=0o700)
            env[key] = str(path)
        env.update(NEBULABOOK_NO_ERROR_DIALOG='1', NEBULABOOK_REQUIRE_CJK_FONT='1',
                   LIBGL_ALWAYS_SOFTWARE='1', RUST_BACKTRACE='1')
        if backend == 'x11':
            env.pop('WAYLAND_DISPLAY', None)
            env['XDG_SESSION_TYPE'] = 'x11'
            launch(['openbox'], 'window-manager', env)
        elif backend == 'wayland':
            env.pop('DISPLAY', None)
            env['XDG_SESSION_TYPE'] = 'wayland'
            env['WAYLAND_DISPLAY'] = 'wayland-nebulabook'
            weston = launch(['weston', '--backend=headless-backend.so', '--use-gl',
                             '--idle-time=0', '--socket=wayland-nebulabook', '--no-config'],
                            'compositor', env)
            socket_path = Path(env['XDG_RUNTIME_DIR']) / env['WAYLAND_DISPLAY']
            wait_for(lambda: socket_path.exists() or weston.poll() is not None,
                     'Weston did not create its Wayland socket')
            if weston.poll() is not None:
                raise RuntimeError('Weston exited before startup: ' + read_log('compositor'))
        else:
            raise ValueError('Unknown backend')

        def app_start(name):
            process = launch([binary], name, env)
            wait_for(lambda: process.poll() is not None or
                     ('Nebulabook renderer initialized:' in read_log(name) and
                      'Nebulabook CJK font initialized: true' in read_log(name)),
                     f'{backend} renderer/font initialization timed out')
            if process.poll() is not None:
                raise RuntimeError('Application exited: ' + read_log(name))
            time.sleep(0.5)
            if process.poll() is not None:
                raise RuntimeError('Application exited after initialization: ' + read_log(name))
            return process

        app = app_start('app')
        checks = ['renderer-initialized', 'cjk-glyphs-present', 'process-alive']
        if backend == 'x11':
            # xdotool inherits DISPLAY from xvfb-run. Input is handled by the real
            # window/event loop, including the application's raw event ordering.
            window = command('xdotool', 'search', '--sync', '--onlyvisible', '--pid', str(app.pid)).splitlines()[0]
            command('xdotool', 'windowactivate', '--sync', window)
            command('xdotool', 'key', '--clearmodifiers', 'ctrl+n')
            time.sleep(0.3)
            command('xdotool', 'type', '--clearmodifiers', '--delay', '15', 'Linux smoke title')
            command('xdotool', 'key', 'Tab')
            command('xdotool', 'type', '--clearmodifiers', '--delay', '15', 'Linux smoke body 123')
            command('xdotool', 'key', '--clearmodifiers', 'ctrl+s')
            data = Path(env['XDG_DATA_HOME']) / 'nebulanotepad' / 'notebook.json'

            def saved():
                if not data.exists():
                    return False
                notebook = json.loads(data.read_text())
                return len(notebook['notes']) == 1 and notebook['notes'][0]['title'] == 'Linux smoke title' and notebook['notes'][0]['content'] == 'Linux smoke body 123'

            wait_for(saved, 'Real X11 keyboard input did not persist correctly')
            assert data.stat().st_mode & 0o777 == 0o600
            assert data.parent.stat().st_mode & 0o777 == 0o700
            if shutil.which('import'):
                subprocess.run(['import', '-window', window, str(output / 'x11-editor.png')], check=True)
            command('xdotool', 'key', '--clearmodifiers', 'alt+F4')
            wait_for(lambda: app.poll() is not None, 'Normal close did not exit')
            assert app.returncode == 0
            before = data.read_bytes()
            app = app_start('reopen')
            assert data.read_bytes() == before
            checks += ['new-edit-save', 'private-data-permissions', 'normal-close', 'reopen-preserves-data']
            window = command('xdotool', 'search', '--sync', '--onlyvisible', '--pid', str(app.pid)).splitlines()[0]
            command('xdotool', 'windowactivate', '--sync', window)
            command('xdotool', 'key', '--clearmodifiers', 'ctrl+n')
            time.sleep(0.3)
            command('xdotool', 'type', '--clearmodifiers', '--delay', '15', 'Unsaved conflict draft')
            # Simulate a non-cooperating external editor while a draft exists.
            external = data.read_bytes() + b'\n'
            data.write_bytes(external)
            command('xdotool', 'key', '--clearmodifiers', 'alt+F4')
            time.sleep(1.2)
            assert app.poll() is None, 'Save conflict must block close and preserve the draft'
            assert data.read_bytes() == external, 'Save conflict overwrote external data'
            if shutil.which('import'):
                subprocess.run(['import', '-window', window, str(output / 'x11-conflict.png')], check=True)
            checks += ['save-conflict-blocks-close', 'external-data-not-overwritten']
        (output / f'{backend}-result.json').write_text(json.dumps({
            'backend': backend, 'binary': binary, 'binary_sha256': hashlib.sha256(Path(binary).read_bytes()).hexdigest(), 'checks': checks,
            'rendering': 'Mesa software / virtual display',
            'not_covered': ['physical GPU', 'all distributions', 'IME compositions', 'desktop portal chooser'],
        }, indent=2) + '\n')
        print(f'{backend}: ' + ', '.join(checks))
finally:
    for process in reversed(processes):
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
    for handle in handles:
        handle.close()
