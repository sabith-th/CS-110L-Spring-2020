use crate::dwarf_data::DwarfData;
use nix::sys::ptrace;
use nix::sys::signal;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use std::collections::HashMap;
use std::mem::size_of;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};

#[derive(Debug, Clone)]
struct Breakpoint {
    addr: usize,
    orig_byte: u8,
}

pub enum Status {
    /// Indicates inferior stopped. Contains the signal that stopped the process, as well as the
    /// current instruction pointer that it is stopped at.
    Stopped(signal::Signal, usize),

    /// Indicates inferior exited normally. Contains the exit status code.
    Exited(i32),

    /// Indicates the inferior exited due to a signal. Contains the signal that killed the
    /// process.
    Signaled(signal::Signal),
}

/// This function calls ptrace with PTRACE_TRACEME to enable debugging on a process. You should use
/// pre_exec with Command to call this in the child process.
fn child_traceme() -> Result<(), std::io::Error> {
    ptrace::traceme().or(Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "ptrace TRACEME failed",
    )))
}

pub struct Inferior {
    child: Child,
    breakpoints_map: HashMap<usize, Breakpoint>,
}

fn align_addr_to_word(addr: usize) -> usize {
    addr & (-(size_of::<usize>() as isize) as usize)
}

impl Inferior {
    /// Attempts to start a new inferior process. Returns Some(Inferior) if successful, or None if
    /// an error is encountered.
    pub fn new(target: &str, args: &Vec<String>, breakpoints: &Vec<usize>) -> Option<Inferior> {
        // TODO: implement me!
        let mut cmd = Command::new(target);
        cmd.args(args);
        unsafe {
            cmd.pre_exec(child_traceme);
        }
        let child = cmd.spawn().expect("Failed to spawn child process");
        let mut inferior = Inferior {
            child,
            breakpoints_map: HashMap::new(),
        };
        match inferior.wait(None) {
            Ok(_) => {
                for (i, bp) in breakpoints.iter().enumerate() {
                    inferior.set_breakpoint(*bp, i);
                }
                Some(inferior)
            }
            Err(_) => None,
        }
    }

    /// Returns the pid of this inferior.
    pub fn pid(&self) -> Pid {
        nix::unistd::Pid::from_raw(self.child.id() as i32)
    }

    /// Calls waitpid on this inferior and returns a Status to indicate the state of the process
    /// after the waitpid call.
    pub fn wait(&self, options: Option<WaitPidFlag>) -> Result<Status, nix::Error> {
        Ok(match waitpid(self.pid(), options)? {
            WaitStatus::Exited(_pid, exit_code) => Status::Exited(exit_code),
            WaitStatus::Signaled(_pid, signal, _core_dumped) => Status::Signaled(signal),
            WaitStatus::Stopped(_pid, signal) => {
                let regs = ptrace::getregs(self.pid())?;
                Status::Stopped(signal, regs.rip as usize)
            }
            other => panic!("waitpid returned unexpected status: {:?}", other),
        })
    }

    pub fn continue_child(&mut self, debug_data: &DwarfData) {
        match ptrace::cont(self.pid(), None) {
            Ok(()) => {}
            Err(e) => println!("Child errorred while continuing {}", e),
        };
        match self.wait(None) {
            Ok(status) => match status {
                Status::Exited(code) => {
                    println!("Child exited (status {})", code);
                    // self.inferior = None;
                }
                Status::Signaled(signal) => {
                    println!("Child signaled with {}", signal)
                }
                Status::Stopped(signal, rip) => {
                    println!("Child stopped with {}", signal);
                    self.print_stopped_instruction(debug_data, rip);
                }
            },
            Err(e) => println!("Child errorred while waiting {}", e),
        };
    }

    pub fn continue_process(&mut self) -> Result<Status, nix::Error> {
        ptrace::cont(self.pid(), None)?;
        match self.wait(None) {
            Ok(status) => match status {
                Status::Stopped(signal::Signal::SIGTRAP, rip) => {
                    if self.breakpoints_map.contains_key(&(rip - 1)) {
                        self.continue_from_breakpoint(rip - 1)
                    } else {
                        Ok(status)
                    }
                }
                _ => Ok(status),
            },
            Err(e) => Err(e),
        }
    }

    pub fn kill(&mut self) {
        match self.child.kill() {
            Err(e) => println!("Error killing child {}", e),
            Ok(()) => println!("Killed child successfully"),
        }
        match self.child.wait() {
            Ok(status) => println!("Child exited with status {}", status),
            Err(e) => println!("Error exiting child {}", e),
        }
    }

    pub fn print_backtrace(&self, debug_data: &DwarfData) {
        match ptrace::getregs(self.pid()) {
            Ok(regs) => {
                let mut instruction_ptr = regs.rip as usize;
                let mut base_ptr = regs.rbp as usize;
                loop {
                    let function = debug_data
                        .get_function_from_addr(instruction_ptr)
                        .unwrap_or("Unable to get function name".to_string());
                    let line = debug_data
                        .get_line_from_addr(instruction_ptr)
                        .unwrap_or_default();
                    println!("{} ({}:{})", function, line.file, line.number);
                    if function == "main" {
                        break;
                    }
                    instruction_ptr =
                        match ptrace::read(self.pid(), (base_ptr + 8) as ptrace::AddressType) {
                            Ok(iptr) => iptr as usize,
                            Err(e) => {
                                println!("Unable to read rip memory at {} {}", base_ptr + 8, e);
                                break;
                            }
                        };
                    base_ptr = match ptrace::read(self.pid(), base_ptr as ptrace::AddressType) {
                        Ok(bptr) => bptr as usize,
                        Err(e) => {
                            println!("Unable to read rbp memory at {} {}", base_ptr, e);
                            break;
                        }
                    }
                }
            }
            Err(e) => println!("Unable to get register value {}", e),
        }
    }

    pub fn print_stopped_instruction(&self, debug_data: &DwarfData, rip: usize) {
        let function = debug_data
            .get_function_from_addr(rip)
            .unwrap_or("Unable to get function name".to_string());
        let line = debug_data.get_line_from_addr(rip).unwrap_or_default();
        println!("Stopped at {} ({}:{})", function, line.file, line.number);
    }

    fn write_byte(&mut self, addr: usize, val: u8) -> Result<u8, nix::Error> {
        let aligned_addr = align_addr_to_word(addr);
        let byte_offset = addr - aligned_addr;
        let word = ptrace::read(self.pid(), aligned_addr as ptrace::AddressType)? as u64;
        let orig_byte = (word >> 8 * byte_offset) & 0xff;
        let masked_word = word & !(0xff << 8 * byte_offset);
        let updated_word = masked_word | ((val as u64) << 8 * byte_offset);
        ptrace::write(
            self.pid(),
            aligned_addr as ptrace::AddressType,
            updated_word as *mut std::ffi::c_void,
        )?;
        Ok(orig_byte as u8)
    }

    pub fn set_breakpoint(&mut self, addr: usize, i: usize) {
        match self.write_byte(addr, 0xcc) {
            Ok(orig_byte) => {
                self.breakpoints_map
                    .insert(addr, Breakpoint { addr, orig_byte });
                println!("Set breakpoint {} at {}", i, addr);
            }
            Err(e) => println!("Error adding breakpoint {}", e),
        }
    }

    pub fn continue_from_breakpoint(&mut self, bp: usize) -> Result<Status, nix::Error> {
        let _ = ptrace::step(self.pid(), signal::Signal::SIGTRAP);
        match self.wait(None) {
            Ok(status) => match status {
                Status::Stopped(signal::Signal::SIGTRAP, _) => {
                    self.write_byte(bp, 0xcc)
                        .expect("Error setting back break point");
                    ptrace::cont(self.pid(), None)?;
                    match self.wait(None) {
                        Ok(status) => match status {
                            Status::Stopped(signal::Signal::SIGTRAP, rip) => {
                                if self.breakpoints_map.contains_key(&(rip - 1)) {
                                    self.write_byte(
                                        rip - 1,
                                        self.breakpoints_map.get(&(rip - 1)).unwrap().orig_byte,
                                    )
                                    .unwrap();
                                    ptrace::getregs(self.pid()).unwrap().rip = (rip - 1) as u64;
                                }
                                ptrace::cont(self.pid(), None)?;
                                self.wait(None)
                            }
                            _ => Ok(status),
                        },
                        Err(e) => Err(e),
                    }
                }
                _ => Ok(status),
            },
            Err(e) => Err(e),
        }
    }
}
