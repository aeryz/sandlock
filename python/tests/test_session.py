"""wait() returns with the main process; shutdown() ends the session."""
import os
import threading
import time

import pytest

from sandlock import Sandbox, ExitStatus, ExitReason, StdioMode


def policy(**kw):
    readable = ["/usr", "/lib", "/bin", "/etc", "/proc", "/dev"]
    if os.path.exists("/lib64"):
        readable.append("/lib64")
    return Sandbox(fs_readable=readable, **kw)


def alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False


def gone(pid, seconds=5):
    deadline = time.monotonic() + seconds
    while alive(pid):
        if time.monotonic() > deadline:
            return False
        time.sleep(0.01)
    return True


def test_wait_keeps_jobs_until_shutdown():
    with policy().popen(["sh", "-c", "sleep 60 & echo $!; exit 7"], stdout=StdioMode.PIPED) as proc:
        job = int(proc.stdout.readline())
        status = proc.wait(timeout=5)
        assert isinstance(status, ExitStatus) and status.exit_code == 7
        assert alive(job) and proc.process_groups()
        assert proc.shutdown().exit_code == 7
        assert gone(job)


def test_join_waits_for_jobs_and_returns_mains_status():
    with policy().popen(["sh", "-c", "sleep 1 & echo $!; exit 7"], stdout=StdioMode.PIPED) as proc:
        job = int(proc.stdout.readline())
        with pytest.raises(TimeoutError):
            proc.join(timeout=0.1)
        assert alive(job)
        assert proc.join(timeout=5).exit_code == 7
        assert not alive(job) and not proc.process_groups()
        assert proc.shutdown().exit_code == 7


def test_sandbox_join_waits_for_jobs():
    sb = policy()
    sb.spawn(["sh", "-c", "sleep 0.5 & exit 3"])
    assert sb.join(timeout=5).exit_code == 3
    assert not sb.process_groups()
    assert sb.shutdown().exit_code == 3


def test_wait_timeout_keeps_the_session():
    sb = policy()
    sb.spawn(["sleep", "60"])
    with pytest.raises(TimeoutError):
        sb.wait(timeout=0.05)
    assert sb.process_groups()
    sb.shutdown()


def test_kill_from_another_thread_ends_a_blocked_wait():
    sb = policy()
    sb.spawn(["sleep", "60"])
    threading.Timer(0.1, sb.kill).start()
    status = sb.wait(timeout=5)
    assert not status.success
    assert status.reason in (ExitReason.KILLED, ExitReason.SIGNALED)
    sb.shutdown()


def test_shutdown_during_a_wait_ends_it():
    sb = policy()
    sb.spawn(["sleep", "60"])
    waiter = threading.Thread(target=sb.wait)
    waiter.start()
    time.sleep(0.1)
    sb.shutdown()
    waiter.join(timeout=5)
    assert not waiter.is_alive()


def test_context_exit_ends_jobs_left_after_wait(tmp_path):
    pidfile = tmp_path / "job"
    with policy(fs_writable=[str(tmp_path)]) as sb:
        sb.spawn(["sh", "-c", f"sleep 60 & echo $! > {pidfile}"])
        sb.wait(timeout=5)
        job = int(pidfile.read_text())
        assert alive(job)
    assert gone(job)


def test_run_timeout_reports_timeout():
    result = policy().run(["sleep", "60"], timeout=0.2)
    assert result.reason == ExitReason.TIMEOUT
    assert not result.success
