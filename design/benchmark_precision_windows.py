"""Read-only process measurements. Starts and terminates only its own temporary-profile children."""
import ctypes as C
from ctypes import wintypes as W
import json, os, statistics, subprocess, sys, tempfile, time
from pathlib import Path

def fn(dll,name,args,result):
    f=getattr(C.WinDLL(dll,use_last_error=True),name);f.argtypes=args;f.restype=result;return f
idle=fn('user32','WaitForInputIdle',[W.HANDLE,W.DWORD],W.DWORD)
times=fn('kernel32','GetProcessTimes',[W.HANDLE,*[C.POINTER(W.FILETIME)]*4],W.BOOL)
foreground=fn('user32','GetForegroundWindow',[],W.HWND)
window_pid=fn('user32','GetWindowThreadProcessId',[W.HWND,C.POINTER(W.DWORD)],W.DWORD)
class Memory(C.Structure):
    _fields_=[('cb',W.DWORD),('faults',W.DWORD),*[(n,C.c_size_t) for n in ('peak_working_set','working_set','peak_paged_pool','paged_pool','peak_nonpaged_pool','nonpaged_pool','pagefile','peak_pagefile','private')]]
mem=fn('psapi','GetProcessMemoryInfo',[W.HANDLE,C.POINTER(Memory),W.DWORD],W.BOOL)
def cpu(h):
    values=[W.FILETIME() for _ in range(4)]
    if not times(h,*map(C.byref,values)): raise C.WinError(C.get_last_error())
    return sum((v.dwHighDateTime<<32)+v.dwLowDateTime for v in values[2:])/1e7
def focused(pid):
    value=W.DWORD();window_pid(foreground(),C.byref(value));return value.value==pid
results=[]
for label,binary,background in [('previous-orbital',sys.argv[1],'orbital'),('precision-orbital',sys.argv[2],'orbital'),('precision-off',sys.argv[2],'off')]:
    for trial in range(3):
        with tempfile.TemporaryDirectory(prefix='aeros-precision-measure-') as d:
            root=Path(d);(root/'project').mkdir();(root/'profile').mkdir()
            (root/'profile/editor.json').write_text(json.dumps({'identity':{'background':background,'fps':20}}))
            start=time.perf_counter();p=subprocess.Popen([binary,str(root/'project')],env={**os.environ,'AEROS_CONFIG_DIR':str(root/'profile')})
            try:
                ready=idle(int(p._handle),10000);startup=(time.perf_counter()-start)*1000
                if ready!=0: raise RuntimeError(f'input idle={ready}')
                time.sleep(1.5);focus_before=focused(p.pid);t0=time.perf_counter();c0=cpu(int(p._handle));time.sleep(1.0);elapsed=time.perf_counter()-t0;usage=(cpu(int(p._handle))-c0)/elapsed*100;focus_after=focused(p.pid)
                memory=Memory();memory.cb=C.sizeof(memory)
                if not mem(int(p._handle),C.byref(memory),memory.cb):raise C.WinError(C.get_last_error())
                results.append({'case':label,'trial':trial+1,'working_set_mib':memory.working_set/1048576,'cpu_percent_of_one_core':usage,'wait_for_input_idle_ms':startup,'focus_before':focus_before,'focus_after':focus_after,'measurement_seconds':elapsed})
            finally:
                if p.poll() is None:p.terminate();p.wait(timeout=10)
output={'method':'3 independent launches/case; 1.5s warmup; 1s CPU window; own temporary profile; no window/input automation','warning':'CPU is percent of one core. Compare matching focus states only; WaitForInputIdle does not measure first fully presented frame. Physical input/GPU timing not measured.','samples':results}
Path(sys.argv[3]).write_text(json.dumps(output,indent=2));print(json.dumps(output))
