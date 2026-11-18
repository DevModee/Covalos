import ctypes as C
from ctypes import wintypes as W
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import time

def function(dll, name, args, result):
    fn = getattr(C.WinDLL(dll, use_last_error=True), name)
    fn.argtypes = args
    fn.restype = result
    return fn

wait_idle = function("user32", "WaitForInputIdle", [W.HANDLE, W.DWORD], W.DWORD)
times = function("kernel32", "GetProcessTimes", [W.HANDLE, *[C.POINTER(W.FILETIME)] * 4], W.BOOL)

class Memory(C.Structure):
    _fields_ = [
        ("cb", W.DWORD), ("faults", W.DWORD),
        *[(name, C.c_size_t) for name in (
            "peak_working_set", "working_set", "peak_paged_pool", "paged_pool",
            "peak_nonpaged_pool", "nonpaged_pool", "pagefile", "peak_pagefile",
            "private",
        )],
    ]

memory = function("psapi", "GetProcessMemoryInfo", [W.HANDLE, C.POINTER(Memory), W.DWORD], W.BOOL)

def cpu_time(handle):
    values = [W.FILETIME() for _ in range(4)]
    if not times(handle, *map(C.byref, values)):
        raise C.WinError(C.get_last_error())
    return sum((x.dwHighDateTime << 32) + x.dwLowDateTime for x in values[2:]) / 10_000_000

def sample(binary, project, config):
    args = [str(binary), str(project)] if project else [str(binary)]
    begin = time.perf_counter()
    with subprocess.Popen(args, env={**os.environ, "AEROS_CONFIG_DIR": str(config)}) as process:
        try:
            handle = int(process._handle)
            if wait_idle(handle, 10_000) != 0 or process.poll() is not None:
                raise RuntimeError(f"{binary} did not reach input idle: {process.poll()}")
            startup = (time.perf_counter() - begin) * 1000
            time.sleep(1.0)
            cpu_start, wall_start = cpu_time(handle), time.perf_counter()
            time.sleep(1.3)
            cpu_percent = 100 * (cpu_time(handle) - cpu_start) / (time.perf_counter() - wall_start)
            values = Memory()
            values.cb = C.sizeof(values)
            if not memory(handle, C.byref(values), values.cb):
                raise C.WinError(C.get_last_error())
            return {
                "input_idle_ms": startup,
                "working_set_mib": values.working_set / 1024**2,
                "private_mib": values.private / 1024**2,
                "cpu_pct_of_one_core": cpu_percent,
            }
        finally:
            process.terminate()

def main():
    original, current, project, config = map(lambda s: Path(s).resolve(), sys.argv[1:])
    # Warm the file cache, then interleave the two executables.
    for binary in (original, current):
        sample(binary, project, config)
    points = {}
    for _ in range(3):
        for label, binary in (("V2", original), ("V3", current)):
            for scenario, root in (("home", None), ("workspace", project)):
                points.setdefault((label, scenario), []).append(sample(binary, root, config))
    print(json.dumps({
        f"{label}_{scenario}": {
            key: round(statistics.median(item[key] for item in results), 2)
            for key in results[0]
        }
        for (label, scenario), results in points.items()
    }, indent=2))

if __name__ == "__main__":
    main()
