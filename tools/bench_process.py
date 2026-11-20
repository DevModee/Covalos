"""Reproducible Windows process benchmark for COVALOS (and legacy AEROS comparison).

Starts and terminates only its own child processes, each with an isolated temporary
configuration profile (`COVALOS_CONFIG_DIR` + `AEROS_CONFIG_DIR`) and empty project.

Measures CPU (% of one core and peak 0.5 s burst), memory (working set & private MiB),
Windows GPU Engine utilization (when available), window resolution, foreground focus
verification, idle vs. active mouse-sweep vs. minimized/background vs. background-off.

Usage:
    python -I tools/bench_process.py <previous_exe> <current_exe> <output_json>
"""
import ctypes as C
from ctypes import wintypes as W
import json
import math
import os
import platform
import subprocess
import sys
import tempfile
import time
from pathlib import Path


def winfn(dll, name, args, result):
    f = getattr(C.WinDLL(dll, use_last_error=True), name)
    f.argtypes = args
    f.restype = result
    return f


wait_input_idle = winfn("user32", "WaitForInputIdle", [W.HANDLE, W.DWORD], W.DWORD)
get_process_times = winfn("kernel32", "GetProcessTimes", [W.HANDLE, *[C.POINTER(W.FILETIME)] * 4], W.BOOL)
get_foreground = winfn("user32", "GetForegroundWindow", [], W.HWND)
set_foreground = winfn("user32", "SetForegroundWindow", [W.HWND], W.BOOL)
show_window = winfn("user32", "ShowWindow", [W.HWND, C.c_int], W.BOOL)
get_window_pid = winfn("user32", "GetWindowThreadProcessId", [W.HWND, C.POINTER(W.DWORD)], W.DWORD)
is_window_visible = winfn("user32", "IsWindowVisible", [W.HWND], W.BOOL)
get_window_rect = winfn("user32", "GetWindowRect", [W.HWND, C.POINTER(W.RECT)], W.BOOL)
get_cursor_pos = winfn("user32", "GetCursorPos", [C.POINTER(W.POINT)], W.BOOL)
set_cursor_pos = winfn("user32", "SetCursorPos", [C.c_int, C.c_int], W.BOOL)
get_system_metrics = winfn("user32", "GetSystemMetrics", [C.c_int], C.c_int)
keybd_event = winfn("user32", "keybd_event", [W.BYTE, W.BYTE, W.DWORD, C.c_size_t], None)
ENUM_PROC = C.WINFUNCTYPE(W.BOOL, W.HWND, W.LPARAM)
enum_windows = winfn("user32", "EnumWindows", [ENUM_PROC, W.LPARAM], W.BOOL)

SW_MINIMIZE = 6
SW_RESTORE = 9
VK_MENU = 0x12
KEYEVENTF_KEYUP = 0x0002


class MemoryCounters(C.Structure):
    _fields_ = [
        ("cb", W.DWORD),
        ("faults", W.DWORD),
        *[
            (name, C.c_size_t)
            for name in (
                "peak_working_set",
                "working_set",
                "peak_paged_pool",
                "paged_pool",
                "peak_nonpaged_pool",
                "nonpaged_pool",
                "pagefile",
                "peak_pagefile",
                "private",
            )
        ],
    ]


get_process_memory = winfn(
    "psapi", "GetProcessMemoryInfo", [W.HANDLE, C.POINTER(MemoryCounters), W.DWORD], W.BOOL
)


def cpu_seconds(handle: int) -> float:
    vals = [W.FILETIME() for _ in range(4)]
    if not get_process_times(handle, *map(C.byref, vals)):
        raise C.WinError(C.get_last_error())
    return sum((x.dwHighDateTime << 32) + x.dwLowDateTime for x in vals[2:]) / 1e7


def find_main_hwnd(pid: int, timeout: float = 8.0) -> int | None:
    deadline = time.perf_counter() + timeout
    while time.perf_counter() < deadline:
        found = []

        def callback(hwnd, _):
            if is_window_visible(hwnd):
                wpid = W.DWORD()
                get_window_pid(hwnd, C.byref(wpid))
                if wpid.value == pid:
                    rect = W.RECT()
                    if get_window_rect(hwnd, C.byref(rect)) and (rect.right - rect.left) >= 400:
                        found.append(hwnd)
                        return False
            return True

        enum_windows(ENUM_PROC(callback), 0)
        if found:
            return found[0]
        time.sleep(0.05)
    return None


def focus_hwnd(hwnd: int) -> bool:
    show_window(hwnd, SW_RESTORE)
    set_foreground(hwnd)
    time.sleep(0.25)
    return get_foreground() == hwnd


def is_focused(pid: int) -> bool:
    wpid = W.DWORD()
    get_window_pid(get_foreground(), C.byref(wpid))
    return wpid.value == pid


def window_rect_dict(hwnd: int) -> dict:
    rect = W.RECT()
    get_window_rect(hwnd, C.byref(rect))
    return {
        "left": rect.left,
        "top": rect.top,
        "width": rect.right - rect.left,
        "height": rect.bottom - rect.top,
    }


def query_hardware() -> dict:
    info = {
        "os": platform.platform(),
        "logical_cores": os.cpu_count(),
        "screen_resolution": f"{get_system_metrics(0)}x{get_system_metrics(1)}",
        "cpu": platform.processor(),
        "gpu": [],
    }
    try:
        ps = subprocess.run(
            [
                "powershell",
                "-NoProfile",
                "-Command",
                "(Get-CimInstance Win32_Processor | Select-Object -First 1 -ExpandProperty Name); '---'; (Get-CimInstance Win32_VideoController | Select-Object -ExpandProperty Name)",
            ],
            capture_output=True,
            text=True,
            timeout=6,
        )
        if ps.returncode == 0:
            parts = ps.stdout.split("---")
            if parts[0].strip():
                info["cpu"] = parts[0].strip()
            if len(parts) > 1:
                info["gpu"] = [line.strip() for line in parts[1].splitlines() if line.strip()]
    except Exception:
        pass
    return info


def sample_gpu_percent(pid: int) -> float | None:
    """Best-effort Windows GPU Engine utilization sum for `pid` across 3D/Compute engines."""
    try:
        cmd = (
            f"$c = Get-Counter '\\GPU Engine(pid_{pid}_*)\\Utilization Percentage' -ErrorAction SilentlyContinue; "
            f"if ($c) {{ ($c.CounterSamples | Measure-Object -Property CookedValue -Sum).Sum }}"
        )
        res = subprocess.run(
            ["powershell", "-NoProfile", "-Command", cmd],
            capture_output=True,
            text=True,
            timeout=3,
        )
        text = res.stdout.strip().replace(",", ".")
        if text:
            return round(float(text), 2)
    except Exception:
        pass
    return None


def measure_window(handle: int, duration: float, sweep_rect: dict | None = None) -> tuple[float, float]:
    """Measure average CPU% (of 1 core) and peak 0.5 s CPU% burst over `duration` seconds."""
    steps = max(1, int(round(duration / 0.5)))
    t_start = time.perf_counter()
    c_start = cpu_seconds(handle)
    peak = 0.0
    for step in range(steps):
        st0 = time.perf_counter()
        sc0 = cpu_seconds(handle)
        if sweep_rect is not None:
            # Move cursor smoothly in an oval across the home Dither Field surface.
            cx = sweep_rect["left"] + sweep_rect["width"] * 0.62
            cy = sweep_rect["top"] + sweep_rect["height"] * 0.50
            rx = sweep_rect["width"] * 0.22
            ry = sweep_rect["height"] * 0.24
            for sub in range(25):
                angle = (step * 25 + sub) * 0.22
                x = int(cx + math.cos(angle) * rx)
                y = int(cy + math.sin(angle * 1.3) * ry)
                set_cursor_pos(x, y)
                time.sleep(0.02)
        else:
            time.sleep(0.5)
        dt = time.perf_counter() - st0
        burst = (cpu_seconds(handle) - sc0) / max(dt, 1e-4) * 100.0
        peak = max(peak, burst)
    total_dt = time.perf_counter() - t_start
    avg = (cpu_seconds(handle) - c_start) / max(total_dt, 1e-4) * 100.0
    return round(avg, 2), round(peak, 2)


def run_benchmark(previous_exe: str, current_exe: str, out_path: Path) -> None:
    cases = [
        ("previous-dither-field", previous_exe, {"identity": {"background": "dither", "fps": 24}}),
        ("current-dither-field", current_exe, {"identity": {"background": "dither", "fps": 24}}),
        ("current-dither-bloom", current_exe, {"identity": {"background": "bloom", "fps": 24}}),
        ("current-background-off", current_exe, {"identity": {"background": "off"}}),
        (
            "current-off-reduced-motion",
            current_exe,
            {"identity": {"background": "off"}, "reduce_motion": True},
        ),
    ]

    orig_cursor = W.POINT()
    get_cursor_pos(C.byref(orig_cursor))
    samples = []

    try:
        for label, binary, prefs in cases:
            for trial in range(2):
                with tempfile.TemporaryDirectory(prefix="covalos-bench-") as tmp:
                    root = Path(tmp)
                    (root / "project").mkdir()
                    (root / "profile").mkdir()
                    (root / "profile/editor.json").write_text(
                        json.dumps({**prefs, "language": "english"}), encoding="utf-8"
                    )
                    env = {
                        **os.environ,
                        "COVALOS_CONFIG_DIR": str(root / "profile"),
                        "AEROS_CONFIG_DIR": str(root / "profile"),
                    }
                    start = time.perf_counter()
                    proc = subprocess.Popen([binary, str(root / "project")], env=env)
                    try:
                        ready = wait_input_idle(int(proc._handle), 10000)
                        startup_ms = (time.perf_counter() - start) * 1000.0
                        hwnd = find_main_hwnd(proc.pid)
                        if hwnd is not None:
                            focus_hwnd(hwnd)
                        # Wait past Luma's 8 s startup wake window so idle isolates background cost cleanly.
                        time.sleep(max(0.0, 9.0 - (time.perf_counter() - start)))
                        focus_idle_before = is_focused(proc.pid)
                        rect = window_rect_dict(hwnd) if hwnd else {}

                        # Window A: Steady-state foreground idle, no pointer interaction (4.0 s)
                        idle_cpu, idle_peak = measure_window(int(proc._handle), 4.0)
                        gpu_idle = sample_gpu_percent(proc.pid)
                        focus_idle_after = is_focused(proc.pid)

                        # Window B: Foreground active, continuous mouse motion across canvas (3.0 s)
                        mouse_cpu, mouse_peak = measure_window(
                            int(proc._handle), 3.0, sweep_rect=rect if rect else None
                        )
                        focus_mouse_after = is_focused(proc.pid)

                        # Window C: Minimized / background state (3.0 s)
                        if hwnd is not None:
                            show_window(hwnd, SW_MINIMIZE)
                            time.sleep(1.0)
                        bg_cpu, bg_peak = measure_window(int(proc._handle), 3.0)

                        mem = MemoryCounters()
                        mem.cb = C.sizeof(mem)
                        get_process_memory(int(proc._handle), C.byref(mem), mem.cb)

                        samples.append(
                            {
                                "case": label,
                                "trial": trial + 1,
                                "window": rect,
                                "focus_idle_before": focus_idle_before,
                                "focus_idle_after": focus_idle_after,
                                "focus_mouse_after": focus_mouse_after,
                                "cpu_idle_pct": idle_cpu,
                                "cpu_idle_peak_pct": idle_peak,
                                "cpu_mouse_pct": mouse_cpu,
                                "cpu_mouse_peak_pct": mouse_peak,
                                "cpu_background_pct": bg_cpu,
                                "cpu_background_peak_pct": bg_peak,
                                "gpu_idle_pct": gpu_idle,
                                "working_set_mib": round(mem.working_set / 1048576, 1),
                                "private_mib": round(mem.private / 1048576, 1),
                                "wait_for_input_idle_ms": round(startup_ms, 1),
                                "input_idle_result": ready,
                            }
                        )
                    finally:
                        if proc.poll() is None:
                            proc.terminate()
                            proc.wait(timeout=10)
    finally:
        set_cursor_pos(orig_cursor.x, orig_cursor.y)

    # Aggregate summary per case
    summary = {}
    for label, _, _ in cases:
        rows = [s for s in samples if s["case"] == label]
        focused_rows = [s for s in rows if s["focus_idle_before"] and s["focus_idle_after"]]
        valid = focused_rows or rows
        avg = lambda k: round(sum(r[k] for r in valid) / len(valid), 2)
        gpu_vals = [r["gpu_idle_pct"] for r in valid if r["gpu_idle_pct"] is not None]
        summary[label] = {
            "focused_trials": f"{len(focused_rows)}/{len(rows)}",
            "cpu_idle_pct": avg("cpu_idle_pct"),
            "cpu_idle_peak_pct": max(r["cpu_idle_peak_pct"] for r in valid),
            "cpu_mouse_pct": avg("cpu_mouse_pct"),
            "cpu_mouse_peak_pct": max(r["cpu_mouse_peak_pct"] for r in valid),
            "cpu_background_pct": avg("cpu_background_pct"),
            "gpu_idle_pct": round(sum(gpu_vals) / len(gpu_vals), 2) if gpu_vals else None,
            "working_set_mib": avg("working_set_mib"),
            "private_mib": avg("private_mib"),
        }

    report = {
        "hardware": query_hardware(),
        "method": (
            "2 launches per case in isolated temp profile & empty project; window forced to "
            "foreground and focus verified; Window A = 4.0 s foreground idle, Window B = 3.0 s "
            "continuous mouse sweep, Window C = 3.0 s minimized background. CPU is % of one logical core."
        ),
        "summary": summary,
        "samples": samples,
    }
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(json.dumps(report["summary"], indent=2))


if __name__ == "__main__":
    if len(sys.argv) != 4:
        sys.exit("Usage: python -I tools/bench_process.py <previous_exe> <current_exe> <output_json>")
    run_benchmark(sys.argv[1], sys.argv[2], Path(sys.argv[3]))
