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
        // This would call the exec syscall directly
        unimplemented!("exec implementation")
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
        // Wait for process to exit
        unimplemented!("wait implementation")
    }
    
    pub fn kill(self) -> Result<(), ProcessError> {
        // Send SIGKILL
        unimplemented!("kill implementation")
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
unsafe fn create_process(_config: &ProcessConfig) -> Result<Process, ProcessError> {
    // This would integrate with the actual fork/clone/exec implementation
    // in src/proc/fork.rs, src/proc/clone.rs, src/proc/exec.rs
    Err(ProcessError::Io)
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
