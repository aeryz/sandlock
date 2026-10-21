package sandlock

import "errors"

// ErrInvalidString is returned when a string passed to the SDK contains an
// interior NUL byte, which cannot cross the C ABI boundary intact.
var ErrInvalidString = errors.New("sandlock: string contains NUL byte")

// ErrNotRunning is returned by *Process lifecycle methods when no process is
// currently running in the handle.
var ErrNotRunning = errors.New("sandlock: process is not running")

// ErrApprovalAnswered is returned when an Approval is resolved twice.
var ErrApprovalAnswered = errors.New("sandlock: approval already answered")

// ErrNotApprovable reports a grant outside the PolicyFn's approvable rules.
var ErrNotApprovable = errors.New("sandlock: outside the approvable rules")

// ErrInvalidRule reports a malformed net_allow rule, or a hostname where only
// IPs and CIDRs are accepted.
var ErrInvalidRule = errors.New("sandlock: invalid net_allow rule")

// ErrFilesystemEventsRequired is returned by PolicyContext.DenyPath when the
// PolicyFn does not subscribe to CategoryFilesystem.
var ErrFilesystemEventsRequired = errors.New("sandlock: policy_fn does not subscribe to filesystem events")

// ErrTimeout is returned by WaitTimeout when the main process is still
// running; the session is left untouched.
var ErrTimeout = errors.New("sandlock: main process has not exited")
