"""Read-only process measurements for AEROS builds.

Starts and terminates only its own children, each with a temporary profile
(AEROS_CONFIG_DIR) and an empty temporary project. Adapted from
design/benchmark_precision_windows.py in the AEROS repository.
Usage: python -I bench.py <previous exe> <current exe> <output json>
"""
import ctypes as C
from ctypes import wintypes as W
import json, os, subprocess, sys, tempfile, time
from pathlib import Path


def fn(dll, name, args, result):
    f = getattr(C.WinDLL(dll, use_last_error=True), name)
    f.argtypes = args
    f.restype = result
    return f


idle = fn('user32', 'WaitForInputIdle', [W.HANDLE, W.DWORD], W.DWORD)
times = fn('kernel32', 'GetProcessTimes', [W.HANDLE, *[C.POINTER(W.FILETIME)] * 4], W.BOOL)
foreground = fn('user32', 'GetForegroundWindow', [], W.HWND)
window_pid = fn('user32', 'GetWindowThreadProcessId', [W.HWND, C.POINTER(W.DWORD)], W.DWORD)


class Memory(C.Structure):
    _fields_ = [('cb', W.DWORD), ('faults', W.DWORD), *[(n, C.c_size_t) for n in (
        'peak_working_set', 'working_set', 'peak_paged_pool', 'paged_pool', 'peak_nonpaged_pool',
        'nonpaged_pool', 'pagefile', 'peak_pagefile', 'private')]]


mem = fn('psapi', 'GetProcessMemoryInfo', [W.HANDLE, C.POINTER(Memory), W.DWORD], W.BOOL)


def cpu(h):
    v = [W.FILETIME() for _ in range(4)]
    if not times(h, *map(C.byref, v)):
        raise C.WinError(C.get_last_error())
    return sum((x.dwHighDateTime << 32) + x.dwLowDateTime for x in v[2:]) / 1e7


def focused(pid):
    value = W.DWORD()
    window_pid(foreground(), C.byref(value))
    return value.value == pid


cases = [
    ('previous-orbital', sys.argv[1], {'identity': {'background': 'orbital', 'fps': 20}}),
    ('current-dither-field', sys.argv[2], {'identity': {'background': 'dither', 'fps': 24}}),
    ('current-orbital', sys.argv[2], {'identity': {'background': 'orbital', 'fps': 20}}),
    ('current-off', sys.argv[2], {'identity': {'background': 'off'}}),
    ('current-off-reduced-motion', sys.argv[2], {'identity': {'background': 'off'}, 'reduce_motion': True}),
]
results = []
for label, binary, prefs in cases:
    for trial in range(3):
        with tempfile.TemporaryDirectory(prefix='aeros-measure-') as d:
            root = Path(d)
            (root / 'project').mkdir()
            (root / 'profile').mkdir()
            (root / 'profile/editor.json').write_text(json.dumps({**prefs, 'language': 'english'}))
            start = time.perf_counter()
            p = subprocess.Popen([binary, str(root / 'project')],
                                 env={**os.environ, 'AEROS_CONFIG_DIR': str(root / 'profile')})
            try:
                ready = idle(int(p._handle), 10000)
                startup = (time.perf_counter() - start) * 1000
                time.sleep(1.5)
                focus_before = focused(p.pid)
                t0 = time.perf_counter()
                c0 = cpu(int(p._handle))
                time.sleep(3.0)
                elapsed = time.perf_counter() - t0
                usage = (cpu(int(p._handle)) - c0) / elapsed * 100
                # Second window after 10 s without input: steady resting state.
                time.sleep(max(0.0, 10.0 - (time.perf_counter() - start)))
                r0 = time.perf_counter()
                rc0 = cpu(int(p._handle))
                time.sleep(3.0)
                rest = (cpu(int(p._handle)) - rc0) / (time.perf_counter() - r0) * 100
                focus_after = focused(p.pid)
                m = Memory()
                m.cb = C.sizeof(m)
                mem(int(p._handle), C.byref(m), m.cb)
                results.append({
                    'case': label, 'trial': trial + 1,
                    'working_set_mib': round(m.working_set / 1048576, 1),
                    'private_mib': round(m.private / 1048576, 1),
                    'cpu_percent_of_one_core': round(usage, 2),
                    'cpu_percent_at_rest_after_10s': round(rest, 2),
                    'wait_for_input_idle_ms': round(startup, 1), 'input_idle_result': ready,
                    'focus_before': focus_before, 'focus_after': focus_after,
                    'measurement_seconds': round(elapsed, 2),
                })
            finally:
                if p.poll() is None:
                    p.terminate()
                    p.wait(timeout=10)
out = {
    'method': '3 launches per case; own temporary profile and empty project; CPU window A from 1.5 s to 4.5 s after input-idle, window B from 10 s to 13 s after launch with no input; no input automation',
    'warning': 'CPU is percent of one core. Compare samples with the same focus state only. WaitForInputIdle is not first-presented-frame latency. GPU and physical input latency are not measured.',
    'samples': results,
}
Path(sys.argv[3]).write_text(json.dumps(out, indent=2))
print(json.dumps(out, indent=1))
