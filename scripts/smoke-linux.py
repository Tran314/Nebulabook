#!/usr/bin/env python3
"""Real display-server checks, with synthetic notes and captured native windows.

Screenshot capture is evidence for human review, not an automatic aesthetic,
text-clipping, or material-blur verdict. All application and WM data lives in a
new temporary HOME/XDG directory. There are no app test-only rendering hooks.
"""
import json
from contextlib import contextmanager
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

from smoke_fixtures import (DEMO_TITLE, MISSING_QUERY, SEARCH_QUERY, TRASH_TITLE,
                            notebook, read_notebook, seed_nebula)

binary, backend, output_arg = sys.argv[1:]
output = Path(output_arg)
output.mkdir(parents=True, exist_ok=True)
# A failed rerun must not leave older success evidence for this binary.
(output / f'{backend}-result.json').unlink(missing_ok=True)
if backend == 'x11':
    (output / 'x11-screenshots.json').unlink(missing_ok=True)
if not __debug__:
    raise RuntimeError('Smoke assertions require Python without optimization')
processes = []
handles = []
active_data_dir = None
screenshots = []
binary_sha256 = hashlib.sha256(Path(binary).read_bytes()).hexdigest()


def capture_diagnostics():
    """Only inspect this test's disposable data/display, never a real notebook."""
    if active_data_dir is not None and active_data_dir.exists():
        paths = sorted(active_data_dir.rglob('notebook.nebula'))
        for index, path in enumerate(paths):
            data = path.read_bytes()
            (output / f'{backend}-failure-notebook-{index}{path.suffix}').write_bytes(data[:65536])
    if backend == 'x11' and shutil.which('import'):
        try:
            subprocess.run(['import', '-window', 'root', str(output / 'x11-failure.png')], timeout=10, check=False)
        except (OSError, subprocess.TimeoutExpired):
            pass


@contextmanager
def isolated_workspace():
    with tempfile.TemporaryDirectory(prefix=f'nebulabook-{backend}-') as temporary:
        try:
            yield temporary
        except BaseException:
            capture_diagnostics()
            raise


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


def stop(process):
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()


def read_log(name):
    return (output / f'{backend}-{name}.log').read_text(errors='replace')


def command(*args):
    return subprocess.check_output(args, text=True, timeout=15).strip()


def geometry(window):
    fields = dict(line.split('=', 1) for line in
                  command('xdotool', 'getwindowgeometry', '--shell', window).splitlines())
    return int(fields['WIDTH']), int(fields['HEIGHT'])


def native_window(app):
    # Select the actual PID-owned native client, retaining the WM's decorations.
    window = command('xdotool', 'search', '--sync', '--onlyvisible', '--pid', str(app.pid)).splitlines()[0]
    command('xdotool', 'windowactivate', '--sync', window)
    command('xdotool', 'windowmove', '--sync', window, '32', '32')
    return window


def resize(window, width, height):
    command('xdotool', 'windowsize', '--sync', window, str(width), str(height))
    wait_for(lambda: geometry(window) == (width, height),
             f'Native client did not resize to {width}x{height}')
    time.sleep(0.3)


def capture(window, name, expected, *, scale=1):
    # Required, never silently skip screenshot evidence if ImageMagick is absent.
    if not shutil.which('import') or not shutil.which('identify'):
        raise RuntimeError('ImageMagick import and identify are required for visual evidence')
    # Move the pointer outside the client to suppress hover tooltips in evidence.
    command('xdotool', 'mousemove', '2500', '1750')
    time.sleep(0.6)
    path = output / f'x11-{name}.png'
    subprocess.run(['import', '-frame', '-window', window, str(path)], check=True, timeout=15)
    width, height, colors = map(int, command('identify', '-format', '%w %h %k', str(path)).split())
    client = geometry(window)
    assert width >= client[0] and height >= client[1], 'Screenshot omitted native client pixels'
    assert colors > 8, 'Captured frame is unexpectedly uniform'
    evidence = {
        'file': path.name, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
        'image_pixels': [width, height], 'native_client_pixels': list(client),
        'logical_client_size': [n / scale for n in client], 'winit_scale_factor': scale,
        'expected_scene_for_human_review': expected,
        'automated_image_checks': ['readable-png', 'contains-client-size', 'more-than-eight-colors'],
    }
    screenshots.append(evidence)
    # Persist incrementally so useful frames remain documented if a later check fails.
    (output / 'x11-screenshots.json').write_text(json.dumps({
        'binary_sha256': binary_sha256,
        'source': 'Actual Rust application, Xvfb / Openbox / Mesa, ImageMagick capture including native frame',
        'synthetic_data_only': True, 'screenshots': screenshots,
        'data_provenance': {
            'before-input/editor/conflict': 'Fresh app workspace, actual keyboard editing and app saves',
            'demo-*': 'Authenticated .nebula visual fixture; app performs navigation, Restore and New Note',
        },
        'review_required': ['Chinese glyph appearance and clipping', 'spacing, contrast and alignment',
                            'correct visible search results / empty states', 'native frame and small-window usability'],
        'not_verified_by_screenshots': ['pixel-perfect design approval', 'OS-level backdrop blur',
                                        'physical GPU', 'macOS or Windows rendering', 'IME compositions',
                                        'mixed-DPI monitor transitions', 'Wayland visual appearance'],
    }, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    return evidence


def search(query):
    command('xdotool', 'key', '--clearmodifiers', 'ctrl+f')
    time.sleep(0.2)
    command('xdotool', 'key', '--clearmodifiers', 'ctrl+a')
    command('xdotool', 'key', '--clearmodifiers', 'BackSpace')
    if query:
        command('xdotool', 'type', '--clearmodifiers', '--delay', '15', query)
    time.sleep(0.3)


def click(window, x, y):
    # Only navigation controls without app shortcuts use client-relative clicks.
    command('xdotool', 'mousemove', '--window', window, str(x), str(y))
    command('xdotool', 'click', '1')
    time.sleep(0.3)


def normal_close(app, window):
    command('xdotool', 'windowactivate', '--sync', window)
    command('xdotool', 'key', '--clearmodifiers', 'alt+F4')
    wait_for(lambda: app.poll() is not None, 'Normal close did not exit')
    assert app.returncode == 0


try:
    with isolated_workspace() as temporary:
        base = Path(temporary)
        env = os.environ.copy()
        # Do not inherit any path that could write diagnostics to a real profile.
        env.pop('NEBULABOOK_STARTUP_LOG', None)
        for key, name in [('HOME', 'home'), ('XDG_DATA_HOME', 'data'), ('XDG_CONFIG_HOME', 'config'),
                          ('XDG_CACHE_HOME', 'cache'), ('XDG_RUNTIME_DIR', 'runtime')]:
            path = base / name
            path.mkdir(mode=0o700)
            env[key] = str(path)
        active_data_dir = Path(env['XDG_DATA_HOME'])
        env.update(NEBULABOOK_NO_ERROR_DIALOG='1', NEBULABOOK_REQUIRE_CJK_FONT='1',
                   LIBGL_ALWAYS_SOFTWARE='1', RUST_BACKTRACE='1')
        if backend == 'x11':
            env.pop('WAYLAND_DISPLAY', None)
            env['XDG_SESSION_TYPE'] = 'x11'
            env['WINIT_X11_SCALE_FACTOR'] = '1'
            launch(['openbox'], 'window-manager', env)
        elif backend == 'wayland':
            env.pop('DISPLAY', None)
            env.pop('WINIT_X11_SCALE_FACTOR', None)
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

        def app_start(name, app_env=None):
            global active_data_dir
            app_env = env if app_env is None else app_env
            active_data_dir = Path(app_env['XDG_DATA_HOME'])
            process = launch([binary], name, app_env)
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
            # Exact keyboard/persistence assertions from the original smoke test
            # are retained. The independent reader authenticates the new format.
            window = native_window(app)
            capture(window, 'before-input', 'New, empty notebook before keyboard input')
            command('xdotool', 'key', '--clearmodifiers', 'ctrl+n')
            time.sleep(0.3)
            command('xdotool', 'type', '--clearmodifiers', '--delay', '15', 'Linux smoke title')
            command('xdotool', 'key', 'Tab')
            command('xdotool', 'type', '--clearmodifiers', '--delay', '15', 'Linux smoke body 123')
            command('xdotool', 'key', '--clearmodifiers', 'ctrl+s')
            data = Path(env['XDG_DATA_HOME']) / 'nebulanotepad' / 'notebook.nebula'

            def saved():
                if not data.exists():
                    return False
                saved_notebook = read_notebook(data, isolated_root=env['XDG_DATA_HOME'])
                return len(saved_notebook['notes']) == 1 and saved_notebook['notes'][0]['title'] == 'Linux smoke title' and saved_notebook['notes'][0]['content'] == 'Linux smoke body 123'

            wait_for(saved, 'Real X11 keyboard input did not persist correctly')
            assert data.stat().st_mode & 0o777 == 0o600
            assert data.parent.stat().st_mode & 0o777 == 0o700
            assert b'Linux smoke title' not in data.read_bytes(), 'Canonical snapshot exposes plaintext title'
            capture(window, 'editor', 'Exact keyboard-written title and body, saved to .nebula')
            normal_close(app, window)
            before = data.read_bytes()
            app = app_start('reopen')
            assert data.read_bytes() == before
            assert saved(), 'Reopened notebook lost the exact keyboard-written title or body'
            checks += ['new-edit-save', 'private-data-permissions', 'normal-close', 'reopen-preserves-data',
                       'nebula-authenticated-exact-content', 'nebula-not-plaintext-json']
            window = native_window(app)
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
            capture(window, 'conflict', 'Visible failed-save/close protection with draft retained')
            checks += ['save-conflict-blocks-close', 'external-data-not-overwritten']
            # The failure scenario is finished. Terminate only its disposable
            # process; do not reuse its conflicted notebook for visual scenarios.
            stop(app)

            def visual_start(name, *, empty=False, scale=1):
                app_env = env.copy()
                scenario = base / name
                scenario.mkdir(mode=0o700)
                for key, folder in [('XDG_DATA_HOME', 'data'), ('XDG_CONFIG_HOME', 'config'),
                                    ('XDG_CACHE_HOME', 'cache')]:
                    path = scenario / folder
                    path.mkdir(mode=0o700)
                    app_env[key] = str(path)
                app_env['WINIT_X11_SCALE_FACTOR'] = str(scale)
                # This initializes visual input, never the keyboard/save result.
                canonical = seed_nebula(app_env['XDG_DATA_HOME'], empty=empty)
                before_open = canonical.read_bytes()
                process = app_start(name, app_env)
                assert canonical.read_bytes() == before_open, 'Opening changed the native visual fixture'
                assert read_notebook(canonical, isolated_root=app_env['XDG_DATA_HOME']) == notebook(empty)
                assert canonical.stat().st_mode & 0o777 == 0o600
                return process, native_window(process), canonical, app_env

            app, window, demo, demo_env = visual_start('visual-demo')
            resize(window, 1000, 700)
            demo_before = demo.read_bytes()
            capture(window, 'demo-populated', f'Chinese notebook: seven active notes, two pinned; selected title {DEMO_TITLE}')
            search(SEARCH_QUERY)
            capture(window, 'demo-search', f'Search {SEARCH_QUERY}: exactly one matching active note in the sidebar')
            search(MISSING_QUERY)
            capture(window, 'demo-no-matches', f'Search {MISSING_QUERY}: no matching notes and a legible empty-result message')
            search('')
            # Native client coordinates for the final glass layout, at 1×.
            # Keep critical edit/save/search assertions on real keyboard paths.
            click(window, 542, 30)
            capture(window, 'demo-trash', f'Trash: two deleted notes; selected {TRASH_TITLE}, with read-only content and restore action')
            assert demo.read_bytes() == demo_before, 'Search / trash navigation unexpectedly changed notes'
            # Restore has no keyboard shortcut. The click is checked against
            # authenticated persisted data, so a missed Trash/Restore control
            # cannot silently pass as the requested visual scenario.
            click(window, 441, 128)
            restored_id = next(n['id'] for n in notebook()['notes'] if n['title'] == TRASH_TITLE)

            def restored():
                saved_notes = read_notebook(demo, isolated_root=demo_env['XDG_DATA_HOME'])['notes']
                return any(n['id'] == restored_id and not n['is_deleted'] for n in saved_notes)

            wait_for(restored, 'Real Trash / Restore interaction did not restore the selected note')
            restored_notes = read_notebook(demo, isolated_root=demo_env['XDG_DATA_HOME'])['notes']
            assert len(restored_notes) == len(notebook()['notes'])
            for original, saved_note in zip(notebook()['notes'], restored_notes):
                if original['id'] == restored_id:
                    assert saved_note['is_deleted'] is False and saved_note['deleted_at'] is None
                    assert saved_note['version'] == original['version'] + 1
                    for key in set(original) - {'is_deleted', 'deleted_at', 'version', 'updated_at'}:
                        assert saved_note[key] == original[key], f'Restore changed {key}'
                else:
                    assert saved_note == original, 'Restore changed an unrelated note'
            capture(window, 'demo-restored', 'After Restore: one remaining deleted note; restored content preserved in the notebook')
            demo_before = demo.read_bytes()
            normal_close(app, window)
            assert demo.read_bytes() == demo_before
            checks += ['native-demo-fixture-preserved', 'visual-navigation-preserves-data',
                       'trash-restore-preserves-content']

            # Fresh process resets search/trash/scroll state through the real UI.
            app = app_start('visual-small', demo_env)
            window = native_window(app)
            resize(window, 640, 420)
            capture(window, 'demo-small-640x420', '640×420 logical client: navigation, editor, toolbar and status remain usable')
            search(MISSING_QUERY)
            capture(window, 'demo-small-no-matches', '640×420 logical client with no search results, without overlapping controls')
            normal_close(app, window)
            assert demo.read_bytes() == demo_before
            checks += ['native-small-window-640x420']

            app, window, empty, empty_env = visual_start('visual-empty', empty=True)
            resize(window, 1000, 700)
            capture(window, 'demo-empty', 'Empty notebook welcome state and create-note action')
            command('xdotool', 'key', '--clearmodifiers', 'ctrl+n')
            time.sleep(0.3)
            capture(window, 'demo-new-note', 'New untitled note from the empty state, with title focus and body placeholder')
            created = read_notebook(empty, isolated_root=empty_env['XDG_DATA_HOME'])
            assert len(created['notes']) == 1
            assert created['notes'][0]['title'] == '' and created['notes'][0]['content'] == ''
            normal_close(app, window)
            checks += ['empty-state-new-note']

            app, window, hidpi, hidpi_env = visual_start('visual-hidpi', scale=2)
            # Verify scaling reached the native window: winit's 640×420 logical
            # minimum must become 1280×840 physical pixels at 2×. This uses the
            # documented WINIT_X11_SCALE_FACTOR, not app-only testing behavior.
            command('xdotool', 'windowsize', '--sync', window, '640', '420')
            wait_for(lambda: geometry(window) == (1280, 840),
                     '2× native minimum was not 1280×840 physical pixels')
            resize(window, 2000, 1400)
            capture(window, 'demo-hidpi-2x', '2× scaling: Chinese populated editor at 1000×700 logical / 2000×1400 physical client pixels', scale=2)
            normal_close(app, window)
            assert read_notebook(hidpi, isolated_root=hidpi_env['XDG_DATA_HOME']) == notebook()
            checks += ['native-hidpi-2x-minimum-verified', 'native-visual-scenes-captured']
        (output / f'{backend}-result.json').write_text(json.dumps({
            'backend': backend, 'binary': binary, 'binary_sha256': binary_sha256, 'checks': checks,
            'rendering': 'Mesa software / virtual display',
            'screenshot_manifest': 'x11-screenshots.json' if backend == 'x11' else None,
            'not_covered': ['physical GPU', 'all distributions', 'IME compositions', 'desktop portal chooser',
                            'automated visual design approval', 'mixed-DPI monitor transitions'],
        }, indent=2) + '\n')
        print(f'{backend}: ' + ', '.join(checks))
finally:
    for process in reversed(processes):
        stop(process)
    for handle in handles:
        handle.close()
