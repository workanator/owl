extern crate nix;
extern crate signal_hook;
extern crate siquery;

use std::env;
use std::net::{SocketAddr, UdpSocket};
use std::process::{self, Command};
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use std::thread;
use std::time;
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;

const UNIX_SIGNAL_EXIT_CODE: i32 = 128;

static CHILD_PID: AtomicU32 = AtomicU32::new(0);
static LAST_SIGNAL: AtomicI32 = AtomicI32::new(0);

fn main() {
    // Do nothing if no command line arguments are passed.
    if env::args().len() < 2 {
        return;
    }

    // Start up facilities
    thread::spawn(listen_signals);
    thread::spawn(deliver_state);

    // Spawn the child process with command line arguments passed.
    std::process::exit(execute_command());
}

fn execute_command() -> i32 {
    let mut child = Command::new(command_name())
        .args(command_args())
        .spawn()
        .expect("failed to execute command");
    CHILD_PID.store(child.id(), Ordering::Relaxed);
    child.wait()
        .expect("failed to retrieve command exit status")
        .code()
        .unwrap_or(UNIX_SIGNAL_EXIT_CODE + LAST_SIGNAL.load(Ordering::Relaxed))
}

fn command_name() -> String {
    env::args().into_iter()
        .skip(1)
        .take(1)
        .collect()
}

fn command_args() -> Vec<String> {
    env::args().into_iter()
        .skip(2)
        .collect()
}

fn listen_signals() {
    let signals = signal_hook::iterator::Signals::new(target_signals())
        .expect("failed to setup signal listener");
    for s in signals.forever() {
        // Save the last signal caught
        LAST_SIGNAL.store(s, Ordering::Relaxed);

        // Propagate the signal to the command process
        let pid = CHILD_PID.load(Ordering::Relaxed);
        if pid > 0 {
            if let Some(sig) = cast_signal(s) {
                let _ = signal::kill(Pid::from_raw(pid as i32), sig);
            }
        }
    }
}

fn target_signals() -> Vec<i32> {
    let all = vec!(
        signal_hook::SIGABRT,
        signal_hook::SIGALRM,
        signal_hook::SIGBUS,
        signal_hook::SIGCHLD,
        signal_hook::SIGCONT,
        signal_hook::SIGFPE,
        signal_hook::SIGHUP,
        signal_hook::SIGILL,
        signal_hook::SIGINT,
        signal_hook::SIGIO,
        signal_hook::SIGKILL,
        signal_hook::SIGPIPE,
        signal_hook::SIGPROF,
        signal_hook::SIGQUIT,
        signal_hook::SIGSEGV,
        signal_hook::SIGSTOP,
        signal_hook::SIGSYS,
        signal_hook::SIGTERM,
        signal_hook::SIGTRAP,
        signal_hook::SIGUSR1,
        signal_hook::SIGUSR2,
        signal_hook::SIGWINCH,
    );

    let mut interest = Vec::new();
    for s in all {
        let mut found = false;
        for f in signal_hook::FORBIDDEN {
            if s == *f {
                found = true;
                break;
            }
        }

        if !found {
            interest.push(s);
        }
    }

    interest
}

fn cast_signal(from: i32) -> Option<Signal> {
    if let Ok(sig) = Signal::from_c_int(from) {
        Some(sig)
    }
    else {
        None
    }
}

fn deliver_state() {
    // Create UDP socket
    let local_addr = SocketAddr::from(([127, 0, 0, 1], 3400));
    if let Ok(socket) = UdpSocket::bind(&local_addr) {
        // Try to connect to remote listener
        let remote_addr = "127.0.0.1:9090";
        if socket.connect(remote_addr).is_err() {
            return;
        }

        // Start sending notifications periodically when child PID is defined
        loop {
            let pid = CHILD_PID.load(Ordering::Relaxed);
            if pid > 0 {
                let info = read_process_info(pid);
                let msg = format!("{}||{}||{}||{}", process::id(), pid, info.name, info.state);
                let _ = socket.send(msg.as_ref());
                thread::sleep(time::Duration::from_secs(1));
            }
        }
    }
}

fn read_process_info(id: u32) -> siquery::tables::ProcessesRow {
    siquery::tables::ProcessesRow::gen_processes_row(format!("{}", id).as_str())
}
