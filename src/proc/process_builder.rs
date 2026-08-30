//! Process Builder Pattern for Unified Process Creation
//!
//! This module provides a builder pattern API for process creation,
//! replacing scattered fork/clone/exec calls with a unified interface.

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::ptr;

/// Builder for creating processes with a fluent API
pub struct ProcessBuilder {
    program: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    cwd: Option<String>,
    stdin_fd: Option<usize>,
    stdout_fd: Option<usize>,
    stderr_fd: Option<usize>,
    clone_flags: u64,
    credentials: Option<Credentials>,
    namespaces: Option<Namespaces>,
    resource_limits: Vec<ResourceLimit>,
}

impl ProcessBuilder {
    /// Create a new process builder for the given program
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
            stdin_fd: None,
            stdout_fd: None,
            stderr_fd: None,
            clone_flags: 0,
            credentials: None,
            namespaces: None,
            resource_limits: Vec::new(),
        }
    }
    
    /// Add an argument to the command line
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }
    
    /// Add multiple arguments
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for arg in args {
            self.args.push(arg.into());
        }
        self
    }
    
    /// Set an environment variable
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }
    
    /// Set multiple environment variables
    pub fn envs<I, K, V>(mut self, vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        for (k, v) in vars {
            self.env.push((k.into(), v.into()));
        }
        self
    }
    
    /// Clear all environment variables (start with empty env)
    pub fn env_clear(mut self) -> Self {
        self.env.clear();
        self
    }
    
    /// Set the working directory
    pub fn current_dir(mut self, dir: impl Into<String>) -> Self {
        self.cwd = Some(dir.into());
        self
    }
    
    /// Set stdin file descriptor
    pub fn stdin(mut self, fd: usize) -> Self {
        self.stdin_fd = Some(fd);
        self
    }
    
    /// Set stdout file descriptor
    pub fn stdout(mut self, fd: usize) -> Self {
        self.stdout_fd = Some(fd);
        self
    }
    
    /// Set stderr file descriptor
    pub fn stderr(mut self, fd: usize) -> Self {
        self.stderr_fd = Some(fd);
        self
    }
    
    /// Set clone flags for fine-grained control
    pub fn clone_flags(mut self, flags: u64) -> Self {
        self.clone_flags = flags;
        self
    }
    
    /// Set process credentials (uid/gid)
    pub fn credentials(mut self, creds: Credentials) -> Self {
        self.credentials = Some(creds);
        self
    }
    
    /// Set namespace configuration
    pub fn namespaces(mut self, ns: Namespaces) -> Self {
        self.namespaces = Some(ns);
        self
    }
    
    /// Add a resource limit
    pub fn resource_limit(mut self, limit: ResourceLimit) -> Self {
        self.resource_limits.push(limit);
        self
    }
    
    /// Spawn the process
    pub fn spawn(self) -> Result<Process, ProcessError> {
        // Validate builder state
        if self.program.is_empty() {
            return Err(ProcessError::InvalidProgram);
        }
        
        // Build process configuration
        let config = ProcessConfig {
            program: self.program,
            args: self.args,
            env: self.env,
            cwd: self.cwd,
            stdin_fd: self.stdin_fd,
            stdout_fd: self.stdout_fd,
            stderr_fd: self.stderr_fd,
            clone_flags: self.clone_flags,
            credentials: self.credentials,
            namespaces: self.namespaces,
            resource_limits: self.resource_limits,
        };
        
        // Call into the actual process creation logic
        unsafe { create_process(&config) }
    }
    
    /// Spawn and wait for the process to complete
    pub fn status(self) -> Result<ExitStatus, ProcessError> {
        let process = self.spawn()?;
        process.wait()
    }
    
    /// Execute, replacing current process (exec variant)
    pub fn exec(self) -> Result<(), ProcessError> {
        // Build argv and envp arrays from the builder configuration
        let argv: Vec<String> = core::iter::once(self.program.clone())
            .chain(self.args.iter().cloned())
            .collect();
        
        let envp: Vec<String> = self.env
            .iter()
            .map(|(k, v)| alloc::format!("{}={}", k, v))
            .collect();
        
        // Convert to C-style string pointers for execve
        let argv_cstr: Vec<alloc::ffi::CString> = argv
            .iter()
            .filter_map(|s| alloc::ffi::CString::new(s.as_str()).ok())
            .collect();
        let argv_ptrs: Vec<*const i8> = argv_cstr.iter().map(|s| s.as_ptr()).collect();
        
        let envp_cstr: Vec<alloc::ffi::CString> = envp
            .iter()
            .filter_map(|s| alloc::ffi::CString::new(s.as_str()).ok())
            .collect();
        let envp_ptrs: Vec<*const i8> = envp_cstr.iter().map(|s| s.as_ptr()).collect();
        
        // Call execve syscall - this replaces the current process image
        let path_cstr = alloc::ffi::CString::new(self.program.as_str()).map_err(|_| ProcessError::InvalidProgram)?;
        let result = crate::proc::exec::sys_execve(
            path_cstr.as_ptr() as usize,
            argv_ptrs.as_ptr() as usize,
            envp_ptrs.as_ptr() as usize,
        );
        
        // If we return, execve failed
        if result < 0 {
            Err(ProcessError::from_errno(-(result as i32)))
        } else {
            // execve only returns on error
            Ok(())
        }
    }
}

/// Process credentials
#[derive(Debug, Clone)]
pub struct Credentials {
    pub uid: u32,
    pub gid: u32,
    pub euid: u32,
    pub egid: u32,
}

impl Credentials {
    pub fn new(uid: u32, gid: u32) -> Self {
        Self {
            uid,
            gid,
            euid: uid,
            egid: gid,
        }
    }
    
    pub fn with_euid(mut self, euid: u32) -> Self {
        self.euid = euid;
        self
    }
    
    pub fn with_egid(mut self, egid: u32) -> Self {
        self.egid = egid;
        self
    }
}

/// Namespace configuration
#[derive(Debug, Clone, Default)]
pub struct Namespaces {
    pub pid_ns: bool,
    pub net_ns: bool,
    pub mount_ns: bool,
    pub uts_ns: bool,
    pub ipc_ns: bool,
    pub user_ns: bool,
    pub cgroup_ns: bool,
}

impl Namespaces {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn pid(mut self) -> Self {
        self.pid_ns = true;
        self
    }
    
    pub fn net(mut self) -> Self {
        self.net_ns = true;
        self
    }
    
    pub fn mount(mut self) -> Self {
        self.mount_ns = true;
        self
    }
}

/// Resource limit
#[derive(Debug, Clone)]
pub struct ResourceLimit {
    pub resource: i32,
    pub soft: u64,
    pub hard: u64,
}

impl ResourceLimit {
    pub fn new(resource: i32, soft: u64, hard: u64) -> Self {
        Self { resource, soft, hard }
    }
}

/// Process configuration (internal)
struct ProcessConfig {
    program: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    cwd: Option<String>,
    stdin_fd: Option<usize>,
    stdout_fd: Option<usize>,
    stderr_fd: Option<usize>,
    clone_flags: u64,
    credentials: Option<Credentials>,
    namespaces: Option<Namespaces>,
    resource_limits: Vec<ResourceLimit>,
}

/// Process error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessError {
    InvalidProgram = 22,
    NotFound = 2,
    PermissionDenied = 13,
    OutOfMemory = 12,
    TooManyProcesses = 11,
    Io = 5,
}

impl ProcessError {
    pub fn from_errno(errno: i32) -> Self {
        match errno {
            2 => ProcessError::NotFound,
            5 => ProcessError::Io,
            11 => ProcessError::TooManyProcesses,
            12 => ProcessError::OutOfMemory,
            13 => ProcessError::PermissionDenied,
            22 => ProcessError::InvalidProgram,
            _ => ProcessError::Io,
        }
    }
}

impl From<ProcessError> for isize {
    fn from(err: ProcessError) -> Self {
        -(err as isize)
    }
}

/// Handle to a spawned process
pub struct Process {
    pid: i32,
}

impl Process {
    fn new(pid: i32) -> Self {
        Self { pid }
    }
    
    pub fn pid(&self) -> i32 {
        self.pid
    }
    
    pub fn wait(self) -> Result<ExitStatus, ProcessError> {
        // Use the wait4 syscall to wait for this specific process
        let mut wstatus: u32 = 0;
        let result = crate::proc::wait::sys_wait4(
            self.pid as isize,
            &mut wstatus as *mut u32 as usize,
            0, // options: blocking wait
            0, // no rusage
        );
        
        if result < 0 {
            return Err(ProcessError::from_errno(-(result as i32)));
        }
        
        // Decode the exit status
        let exit_code = if (wstatus & 0x7f) == 0 {
            // Normal exit: extract high byte
            ((wstatus >> 8) & 0xff) as i32
        } else if (wstatus & 0x7f) != 0x7f {
            // Terminated by signal: use negative signal number
            -((wstatus & 0x7f) as i32)
        } else {
            // Stopped or continued - treat as error for now
            -1
        };
        
        Ok(ExitStatus { code: exit_code })
    }
    
    pub fn kill(self) -> Result<(), ProcessError> {
        use crate::proc::signal::SIGKILL;
        
        // Send SIGKILL to the process using the signal module's sys_kill
        let result = crate::proc::signal::sys_kill(self.pid as isize, SIGKILL);
        
        if result < 0 {
            Err(ProcessError::from_errno(-(result as i32)))
        } else {
            Ok(())
        }
    }
}

/// Process exit status
#[derive(Debug, Clone, Copy)]
pub struct ExitStatus {
    code: i32,
}

impl ExitStatus {
    pub fn code(&self) -> i32 {
        self.code
    }
    
    pub fn success(&self) -> bool {
        self.code == 0
    }
}

/// Internal process creation function
/// 
/// This function implements the core process creation logic by integrating
/// with fork/clone/exec syscalls. It:
/// 1. Forks the current process
/// 2. In the child: applies credentials, namespaces, resource limits, cwd, fds
/// 3. In the child: execs the target program
/// 4. In the parent: returns the child PID
unsafe fn create_process(config: &ProcessConfig) -> Result<Process, ProcessError> {
    use crate::proc::fork_syscall::sys_fork;
    use crate::proc::scheduler;
    
    // Step 1: Fork the current process
    let pid = sys_fork();
    
    if pid < 0 {
        // Fork failed
        return Err(ProcessError::from_errno(-(pid as i32)));
    }
    
    if pid == 0 {
        // We are in the child process
        
        // Apply credentials if specified
        if let Some(ref creds) = config.credentials {
            // Set UID/GID - this would normally be done via sys_setuid/sys_setgid
            // For now, we update the current process's credentials directly
            let current_pid = scheduler::current_pid() as usize;
            scheduler::with_proc_mut(current_pid, |pcb, _pl| {
                pcb.uid = creds.uid;
                pcb.gid = creds.gid;
                pcb.euid = creds.euid;
                pcb.egid = creds.egid;
                pcb.suid = creds.uid;
                pcb.sgid = creds.gid;
            });
        }
        
        // Apply namespace changes if specified
        if let Some(ref ns) = config.namespaces {
            let current_pid = scheduler::current_pid() as usize;
            
            // Build unshare flags based on requested namespaces
            let mut unshare_flags: usize = 0;
            if ns.mount_ns {
                unshare_flags |= crate::proc::namespace::CLONE_NEWNS_VAL;
            }
            if ns.net_ns {
                unshare_flags |= crate::proc::namespace::CLONE_NEWNET;
            }
            if ns.pid_ns {
                unshare_flags |= crate::proc::namespace::CLONE_NEWPID;
            }
            if ns.uts_ns {
                unshare_flags |= crate::proc::namespace::CLONE_NEWUTS;
            }
            if ns.ipc_ns {
                unshare_flags |= crate::proc::namespace::CLONE_NEWIPC;
            }
            if ns.user_ns {
                unshare_flags |= crate::proc::namespace::CLONE_NEWUSER;
            }
            if ns.cgroup_ns {
                unshare_flags |= crate::proc::namespace::CLONE_NEWCGROUP;
            }
            
            if unshare_flags != 0 {
                crate::proc::namespace::sys_unshare(unshare_flags);
            }
        }
        
        // Change working directory if specified
        if let Some(ref cwd) = config.cwd {
            // Convert path to CString and call sys_chdir
            if let Ok(cwd_cstr) = alloc::ffi::CString::new(cwd.as_str()) {
                crate::fs::stat_syscalls::sys_chdir(cwd_cstr.as_ptr() as usize);
            }
        }
        
        // Set up file descriptors for stdin/stdout/stderr
        if let Some(stdin_fd) = config.stdin_fd {
            crate::fs::io_syscalls::sys_dup2(stdin_fd, 0);
        }
        if let Some(stdout_fd) = config.stdout_fd {
            crate::fs::io_syscalls::sys_dup2(stdout_fd, 1);
        }
        if let Some(stderr_fd) = config.stderr_fd {
            crate::fs::io_syscalls::sys_dup2(stderr_fd, 2);
        }
        
        // Apply resource limits if specified
        for limit in &config.resource_limits {
            crate::proc::rlimit::setrlimit_for(
                0, // 0 means current process
                limit.resource as usize,
                limit.soft,
                limit.hard,
            );
        }
        
        // Step 2: Build argv and envp for exec
        let argv: Vec<String> = core::iter::once(config.program.clone())
            .chain(config.args.iter().cloned())
            .collect();
        
        let envp: Vec<String> = config.env
            .iter()
            .map(|(k, v)| alloc::format!("{}={}", k, v))
            .collect();
        
        // Convert to C-style string pointers for execve
        let argv_cstr: Vec<alloc::ffi::CString> = argv
            .iter()
            .filter_map(|s| alloc::ffi::CString::new(s.as_str()).ok())
            .collect();
        let argv_ptrs: Vec<*const i8> = argv_cstr.iter().map(|s| s.as_ptr()).collect();
        
        let envp_cstr: Vec<alloc::ffi::CString> = envp
            .iter()
            .filter_map(|s| alloc::ffi::CString::new(s.as_str()).ok())
            .collect();
        let envp_ptrs: Vec<*const i8> = envp_cstr.iter().map(|s| s.as_ptr()).collect();
        
        // Step 3: Exec the target program
        let path_cstr = match alloc::ffi::CString::new(config.program.as_str()) {
            Ok(cstr) => cstr,
            Err(_) => {
                // Invalid program name - exit child with error
                crate::proc::exit::sys_exit(127);
                unreachable!();
            }
        };
        
        let result = crate::proc::exec::sys_execve(
            path_cstr.as_ptr() as usize,
            argv_ptrs.as_ptr() as usize,
            envp_ptrs.as_ptr() as usize,
        );
        
        // If execve returns, it failed
        if result < 0 {
            crate::proc::exit::sys_exit(127);
        }
        
        // Should never reach here
        unreachable!();
    }
    
    // We are in the parent process - return the child PID
    Ok(Process::new(pid as i32))
}

/// Convenience function for simple process spawning
pub fn spawn(program: impl Into<String>, args: Vec<String>) -> Result<Process, ProcessError> {
    ProcessBuilder::new(program)
        .args(args)
        .spawn()
}

/// Run a command and wait for completion
pub fn run(program: impl Into<String>, args: Vec<String>) -> Result<ExitStatus, ProcessError> {
    ProcessBuilder::new(program)
        .args(args)
        .status()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_builder_pattern() {
        let _builder = ProcessBuilder::new("/bin/sh")
            .arg("-c")
            .arg("echo hello")
            .env("PATH", "/bin:/usr/bin")
            .current_dir("/")
            .stdin(0)
            .stdout(1)
            .stderr(2);
    }
    
    #[test]
    fn test_credentials() {
        let creds = Credentials::new(1000, 1000)
            .with_euid(0)
            .with_egid(0);
        
        assert_eq!(creds.uid, 1000);
        assert_eq!(creds.euid, 0);
    }
    
    #[test]
    fn test_namespaces() {
        let ns = Namespaces::new()
            .pid()
            .net()
            .mount();
        
        assert!(ns.pid_ns);
        assert!(ns.net_ns);
        assert!(ns.mount_ns);
    }
}
