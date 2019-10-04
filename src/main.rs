extern crate nix;
extern crate signal_hook;
extern crate siquery;

#[macro_use]
extern crate lazy_static;

use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::net::{SocketAddr, UdpSocket};
use std::process::{self, Command};
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use std::thread;
use std::time;
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;

const UNIX_SIGNAL_EXIT_CODE: i32 = 128;

lazy_static! {
    static ref CHILD_PID: AtomicU32 = AtomicU32::new(0);
    static ref LAST_SIGNAL: AtomicI32 = AtomicI32::new(0);
    static ref OPT: HashMap<String, String> = collect_opts();
    static ref ARGS: Vec<OsString> = collect_command_args();
}

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
    if let Some(name) = command_name() {
        let mut child = Command::new(name)
            .args(command_args())
            .spawn()
            .expect("failed to execute command");
        CHILD_PID.store(child.id(), Ordering::Relaxed);
        child.wait()
            .expect("failed to retrieve command exit status")
            .code()
            .unwrap_or(UNIX_SIGNAL_EXIT_CODE + LAST_SIGNAL.load(Ordering::Relaxed))
    }
    else {
        0
    }
}

fn collect_opts() -> HashMap<String, String> {
    let opts: Vec<String> = env::args_os().into_iter()
        .skip(1)
        .map(|x| x.to_string_lossy().into())
        .filter(|x: &String| x.starts_with("+"))
        .collect();
    let mut dict: HashMap<String, String> = HashMap::new();

    for opt in opts {
        let mut lossy: String = opt.to_string();
        let _ = lossy.remove(0); // Strip the leading +

        // Find the index of collon and split to name and value
        let parts: Vec<&str> = lossy.splitn(2, ':').collect();
        if parts.len() == 2 {
            dict.insert(parts[0].into(), parts[1].into());
        }
        else {
            dict.insert(parts[0].into(), "".into());
        }
    }

    dict
}

fn collect_command_args() -> Vec<OsString> {
    env::args_os().into_iter()
        .skip(1)
        .filter(|x| !x.to_string_lossy().starts_with("+"))
        .collect()
}

fn command_name() -> Option<OsString> {
    ARGS.iter()
        .take(1)
        .map(|x| x.clone())
        .nth(0)
}

fn command_args() -> Vec<OsString> {
    ARGS.iter()
        .skip(1)
        .map(|x| x.clone())
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
    // Find available port
    let port = get_available_port();
    if port.is_none() {
        return;
    }

    // Create UDP socket
    let local_addr = SocketAddr::from(([0, 0, 0, 0], port.unwrap()));
    if let Ok(socket) = UdpSocket::bind(&local_addr) {
        // Try to connect to remote listener
        let remote_addr = "127.0.0.1:9090";
        //if socket.connect(remote_addr).is_err() {
        //    return;
        //}

        // Start sending notifications periodically when child PID is defined
        loop {
            let pid = CHILD_PID.load(Ordering::Relaxed);
            if pid > 0 {
                let info = read_process_info(pid);

                // Get command name from option or from command line
                let cmd_name: String = if let Some(v) = OPT.get("Name") {
                    v.clone()
                }
                else {
                    info.name
                };

                let msg = format!("{}||{}||{}||{}", process::id(), pid, cmd_name, info.state);
                let _ = socket.send_to(msg.as_ref(), remote_addr);
                thread::sleep(time::Duration::from_secs(1));
            }
        }
    }
}

fn read_process_info(id: u32) -> siquery::tables::ProcessesRow {
    siquery::tables::ProcessesRow::gen_processes_row(format!("{}", id).as_str())
}

fn port_is_available(port: u16) -> bool {
    match UdpSocket::bind(("0.0.0.0", port)) {
        Ok(_) => true,
        Err(_) => false,
    }
}

fn get_available_port() -> Option<u16> {
    (1025..65535).find(|port| port_is_available(*port))
}
