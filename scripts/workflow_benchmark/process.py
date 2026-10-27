"""Bounded subprocess observations; child output never becomes an artifact."""
import os
import signal
import subprocess  # nosec B404 - bounded children use argv without a shell
import threading
import time


def terminate(child):
    if os.name == "posix":
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    else:
        # Do not resolve the cleanup utility through the adapter's PATH.
        system_root = os.environ.get("SystemRoot", "")
        if os.path.isabs(system_root):
            taskkill = os.path.join(system_root, "System32", "taskkill.exe")
            try:
                subprocess.run(  # nosec B603 - fixed utility and integer PID, no shell
                    [taskkill, "/PID", str(child.pid), "/T", "/F"],
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
            except OSError:
                pass  # Fall back to killing the direct child below.
    if child.poll() is None:
        child.kill()
    child.wait()


def execute(argv, root, payload, env, seconds, limit):
    """Return code, bounded stdout and a fixed failure reason, never stderr."""
    if seconds <= 0:
        return None, b"", "deadline"
    try:
        child = subprocess.Popen(  # nosec B603 - operator-selected trusted adapter, no shell
            argv, cwd=root, env=env, stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            start_new_session=os.name == "posix",
        )
    except OSError:
        return None, b"", "spawn"
    oversized = threading.Event()
    done = [threading.Event(), threading.Event()]
    outputs = [bytearray(), bytearray()]

    def drain(pipe, index):
        try:
            while chunk := pipe.read(4096):
                available = max(0, limit - len(outputs[index]))
                outputs[index].extend(chunk[:available])
                if len(chunk) > available:
                    oversized.set()
        finally:
            pipe.close()
            done[index].set()

    for index, pipe in enumerate((child.stdout, child.stderr)):
        threading.Thread(target=drain, args=(pipe, index), daemon=True).start()
    # Requests are small synthetic fixtures; a writer thread keeps a child
    # that refuses stdin inside the same deadline as stdout/stderr.
    def write_request():
        try:
            child.stdin.write(payload)
            child.stdin.close()
        except (OSError, BrokenPipeError):
            pass
    threading.Thread(target=write_request, daemon=True).start()
    deadline = time.monotonic() + seconds
    reason = None
    while child.poll() is None or not all(event.is_set() for event in done):
        if oversized.is_set():
            reason = "output-limit"
            break
        if time.monotonic() >= deadline:
            reason = "deadline"
            break
        time.sleep(0.01)
    if oversized.is_set():
        reason = "output-limit"
    terminate(child)
    for event in done:
        event.wait(0.2)
    return child.returncode, bytes(outputs[0]), reason
