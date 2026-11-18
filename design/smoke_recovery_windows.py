import ctypes, json, os, subprocess, sys, tempfile, time
from pathlib import Path
# Verify only the child launched by this check; preserve other editor windows.
wait_idle=ctypes.windll.user32.WaitForInputIdle
wait_idle.argtypes=[ctypes.c_void_p,ctypes.c_ulong]
wait_idle.restype=ctypes.c_ulong
binary=sys.argv[1]
with tempfile.TemporaryDirectory(prefix='aeros-recovery-smoke-') as d:
    project=Path(d)/'project'; project.mkdir()
    env={**os.environ,'AEROS_CONFIG_DIR':str(Path(d)/'profile')}
    process=subprocess.Popen([binary,str(project)],env=env)
    try:
        idle=wait_idle(int(process._handle),10000)
        if idle!=0 or process.poll() is not None: raise RuntimeError(f'No input idle: {idle}, exit={process.poll()}')
        time.sleep(1)
        windows=[]
        callback_type=ctypes.WINFUNCTYPE(ctypes.c_bool,ctypes.c_void_p,ctypes.c_void_p)
        def visit(hwnd,_):
            pid=ctypes.c_ulong()
            ctypes.windll.user32.GetWindowThreadProcessId(ctypes.c_void_p(hwnd),ctypes.byref(pid))
            if pid.value==process.pid and ctypes.windll.user32.IsWindowVisible(ctypes.c_void_p(hwnd)): windows.append(hwnd)
            return True
        callback=callback_type(visit)
        ctypes.windll.user32.EnumWindows(callback,0)
        if not windows: raise RuntimeError('No visible native window for smoke process')
        for attempt in range(3):
            for hwnd in windows: ctypes.windll.user32.PostMessageW(ctypes.c_void_p(hwnd),0x0010,0,0)
            try:
                process.wait(timeout=2)
                break
            except subprocess.TimeoutExpired: pass
        code=process.wait(timeout=10)
        if code!=0: raise RuntimeError(f'Nonzero exit: {code}')
        print(json.dumps({'release_native_launch':'passed','input_idle':'passed','graceful_close':'passed','exit_code':code}))
    finally:
        if process.poll() is None: process.terminate(); process.wait(timeout=5)
