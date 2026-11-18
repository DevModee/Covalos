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

enum_windows = function("user32", "EnumWindows", [C.WINFUNCTYPE(W.BOOL, W.HWND, W.LPARAM), W.LPARAM], W.BOOL)
window_pid = function("user32", "GetWindowThreadProcessId", [W.HWND, C.POINTER(W.DWORD)], W.DWORD)
foreground = function("user32", "GetForegroundWindow", [], W.HWND)
set_foreground = function("user32", "SetForegroundWindow", [W.HWND], W.BOOL)
show = function("user32", "ShowWindow", [W.HWND, C.c_int], W.BOOL)
visible = function("user32", "IsWindowVisible", [W.HWND], W.BOOL)
iconic = function("user32", "IsIconic", [W.HWND], W.BOOL)
text_length = function("user32", "GetWindowTextLengthW", [W.HWND], C.c_int)
def process_window(pid):
    found = []
    @C.WINFUNCTYPE(W.BOOL, W.HWND, W.LPARAM)
    def visit(hwnd, _):
        value = W.DWORD()
        window_pid(hwnd, C.byref(value))
        if value.value == pid and visible(hwnd) and text_length(hwnd)>0:
            found.append(hwnd)
        return True
    enum_windows(visit, 0)
    return found[0] if found else None

def sample(binary, project, config, mode="active"):
    args = [str(binary), str(project)] if project else [str(binary)]
    begin = time.perf_counter()
    with subprocess.Popen(args, env={**os.environ, "AEROS_CONFIG_DIR": str(config)}) as process:
        try:
            handle = int(process._handle)
            if wait_idle(handle, 10_000) != 0 or process.poll() is not None:
                raise RuntimeError(f"{binary} did not reach input idle: {process.poll()}")
            startup = (time.perf_counter() - begin) * 1000
            hwnd=process_window(process.pid)
            if hwnd:
                if mode=="minimized": show(hwnd,6)
                else: set_foreground(hwnd)
            time.sleep(2.0)
            observed_before=bool(hwnd and foreground()==hwnd)
            cpu_start, wall_start = cpu_time(handle), time.perf_counter()
            time.sleep(1.3)
            cpu_percent = 100 * (cpu_time(handle) - cpu_start) / (time.perf_counter() - wall_start)
            values = Memory()
            values.cb = C.sizeof(values)
            if not memory(handle, C.byref(values), values.cb):
                raise C.WinError(C.get_last_error())
            return {
                "foreground_observed": bool(observed_before and hwnd and foreground()==hwnd),
                "minimized_observed": bool(hwnd and iconic(hwnd)),
                "input_idle_ms": startup,
                "working_set_mib": values.working_set / 1024**2,
                "private_mib": values.private / 1024**2,
                "cpu_pct_of_one_core": cpu_percent,
            }
        finally:
            process.terminate()

def main():
    original,current,project,config=map(lambda s:Path(s).resolve(),sys.argv[1:])
    cases=[("V3_static",original,{"identity":{"background":"minimal","animated":False}},"active"),("Quality_orbital",current,{"identity":{"background":"orbital","animated":True,"fps":20}},"active"),("Quality_10fps",current,{"identity":{"background":"orbital","animated":True,"fps":10}},"active"),("Quality_static",current,{"identity":{"background":"orbital","animated":False}},"active"),("Quality_off",current,{"identity":{"background":"off"}},"active"),("Quality_minimized",current,{"identity":{"background":"orbital","animated":True,"fps":20}},"minimized")]
    profiles={}
    for label,_,prefs,_ in cases:
        profile=config/label;profile.mkdir(parents=True,exist_ok=True);(profile/"editor.json").write_text(json.dumps(prefs));profiles[label]=profile
    for label,binary,_,mode in cases:sample(binary,project,profiles[label],mode)
    points={}
    for _ in range(2):
        for label,binary,_,mode in cases:points.setdefault(label,[]).append(sample(binary,project,profiles[label],mode))
    controlled={}
    for label,rows in points.items():
        valid=[row for row in rows if (row['minimized_observed'] if label=='Quality_minimized' else row['foreground_observed'] and not row['minimized_observed'])]
        controlled[label]={'valid_samples':len(valid),**{key:(round(statistics.median(row[key] for row in valid),2) if valid else None) for key in ('input_idle_ms','working_set_mib','private_mib','cpu_pct_of_one_core')}}
    print(json.dumps({'method':'warmup and two interleaved runs; only verified focus/minimization samples enter medians; CPU is percent of one core over 1.3 seconds; workspace Home','results':controlled,'samples':points},indent=2))
if __name__=='__main__':main()
