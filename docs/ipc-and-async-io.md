# IPC and asynchronous I/O

RustOS combines userspace-service endpoints, SysV/POSIX IPC, scheme-backed pipes, readiness queues, and an io_uring subset. These are full-kernel sources at `08a14f6`; passing host tests does not establish that every path executes correctly in a guest.

## Userspace-service endpoints

`ipc/mod.rs` maintains endpoint ownership and two independent queues: kernel-to-server requests/notifications and server-to-kernel replies. Messages are limited to 64 KiB and each queue to 128 entries.

- **Creation and ownership**: `endpoint_create_for_current` assigns the current PID as owner. User-facing operations check endpoint ownership, and exit cleanup removes the exiting process's endpoints.
- **Request/reply**: Kernel callers use send/receive helpers; user syscalls copy messages through checked user-access helpers. Empty user receive returns `EAGAIN`; a blocking scheduler-backed receive loop is not implemented there.
- **Truncation semantics**: The current receive path copies at most the supplied buffer length after removing a message. Callers must not assume a short buffer preserves the remaining bytes for another receive.
- **Driver/scheme use**: `syscall/driver.rs` and `fs/ipc_proxy_scheme.rs` use endpoint handles for device notification and synchronous scheme proxying. The server lifecycle and recovery semantics still need real execution tests.

`scheme-api` defines shared protocol types and flags. Its host tests validate API behavior, not a running userspace service.

## IPC families

| Source | Facility |
| --- | --- |
| `ipc/key.rs` | IPC key/ID mapping and key helpers |
| `ipc/msg.rs` | SysV message queues |
| `ipc/sem.rs` | SysV semaphore operations and cleanup |
| `ipc/shm.rs` | SysV shared memory attach/detach/control |
| `ipc/mq.rs` | POSIX message queues and notification state |
| `ipc/pipe_scheme.rs` | Scheme-backed anonymous pipe handles |
| `ipc/completeness.rs` | Additional IPC operation coverage/helpers |
| `fs/pipe.rs` | Pipe FD creation and syscall-facing behavior; re-exported as `ipc::pipe` |
| `fs/shm.rs` | Separate filesystem/shared-memory interfaces |

IPC permission structures store owner/creator IDs and mode bits. Capability checks such as `CAP_IPC_OWNER` are still described as integration gaps in the implementation, so do not claim full privilege/namespace semantics.

## Readiness and waits

`sync::WaitQueue` and `PollSource` connect descriptor readiness with poll/epoll and asynchronous waiters. Eventfd, timerfd, signalfd, pipes, and sockets each have distinct state and close/cancellation behavior.

Changing one path requires reviewing registration, readiness recheck, wakeup, deadline, and teardown together. Sharing a wait-queue type does not guarantee consistent blocking semantics across all descriptors.

## io_uring subset

`io_uring/ring.rs` owns ring state and the global table; `syscall.rs` implements setup, enter, and register. SQE/CQE definitions, ring-buffer publication, wait integration, and scheduler support are separate modules.

The operation dispatcher accepts `NOP`, `READ`, `WRITE`, `ACCEPT`, `CONNECT`, and `CLOSE`. Unsupported opcodes return `ENOSYS`, and CLOSE deliberately delegates to the normal process-FD close path.

Registration includes buffer/file lists, eventfd-related operations, and updates. Some IOWQ affinity/max-worker options return success without implementing a corresponding worker configuration; document these as accepted no-ops rather than working worker-pool controls.

`io_uring::epoll::is_epoll_fd` always returns false, and `io_uring/token.rs` is not declared by the root. These compatibility surfaces are not evidence of complete Linux io_uring or epoll integration.

## Validation

The smoke workload performs a pipe payload round trip and emits an IPC assertion marker. It is a real guest-test implementation, but the full runtime gate remains unverified and the host mock suite currently fails before establishing the intended assertions.

Before accepting endpoint or async-I/O support, test ownership rejection, invalid buffers, queue saturation, cancellation, close during wait, interrupted operations, and cleanup after server exit. See [acceptance criteria](acceptance-criteria.md) for the evidence format.
