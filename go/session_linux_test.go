//go:build linux

package sandlock_test

import (
	"bufio"
	"strconv"
	"strings"
	"syscall"
	"testing"
	"time"

	sandlock "github.com/multikernel/sandlock/go"
)

func gone(pid int) bool {
	deadline := time.Now().Add(5 * time.Second)
	for syscall.Kill(pid, 0) == nil {
		if time.Now().After(deadline) {
			return false
		}
		time.Sleep(10 * time.Millisecond)
	}
	return true
}

func TestWaitKeepsJobsUntilShutdown(t *testing.T) {
	requireLandlock(t)
	sb := &sandlock.Sandbox{FSReadable: rootfs}
	p, err := sb.Popen(sandlock.Stdio{Stdout: sandlock.StdioPiped}, "sh", "-c", "sleep 60 & echo $!; exit 7")
	if err != nil {
		t.Fatalf("Popen: %v", err)
	}
	defer p.Close()
	line, err := bufio.NewReader(p.Stdout).ReadString('\n')
	if err != nil {
		t.Fatalf("read job pid: %v", err)
	}
	job, _ := strconv.Atoi(strings.TrimSpace(line))
	st, err := p.Wait()
	if err != nil || st.ExitCode != 7 {
		t.Fatalf("Wait: %+v %v", st, err)
	}
	if syscall.Kill(job, 0) != nil {
		t.Fatal("the job died with main")
	}
	if groups, err := p.ProcessGroups(); err != nil || len(groups) == 0 {
		t.Fatalf("ProcessGroups after Wait: %v %v", groups, err)
	}
	if res, err := p.Shutdown(); err != nil || res.ExitCode != 7 {
		t.Fatalf("Shutdown: %+v %v", res, err)
	}
	if !gone(job) {
		t.Fatal("the job outlived Shutdown")
	}
}

func TestWaitTimeoutKeepsTheSession(t *testing.T) {
	requireLandlock(t)
	sb := &sandlock.Sandbox{FSReadable: rootfs}
	p, err := sb.Spawn("sleep", "60")
	if err != nil {
		t.Fatalf("Spawn: %v", err)
	}
	defer p.Close()
	if _, err := p.WaitTimeout(50 * time.Millisecond); err != sandlock.ErrTimeout {
		t.Fatalf("WaitTimeout: want ErrTimeout, got %v", err)
	}
	if groups, err := p.ProcessGroups(); err != nil || len(groups) == 0 {
		t.Fatalf("a timed-out wait must leave the session: %v %v", groups, err)
	}
}

func TestShutdownEndsABlockedWait(t *testing.T) {
	requireLandlock(t)
	sb := &sandlock.Sandbox{FSReadable: rootfs}
	p, err := sb.Spawn("sleep", "60")
	if err != nil {
		t.Fatalf("Spawn: %v", err)
	}
	done := make(chan error, 1)
	go func() {
		_, werr := p.Wait()
		done <- werr
	}()
	time.Sleep(100 * time.Millisecond)
	if _, err := p.Shutdown(); err != nil {
		t.Fatalf("Shutdown: %v", err)
	}
	select {
	case <-done:
	case <-time.After(5 * time.Second):
		t.Fatal("Shutdown did not end a blocked Wait")
	}
}
