/*
 * Copyright 2019 Andrew "workanator" Bashkatov
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *    http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

///
/// The tool wraps command in child process and sends it's state periodically
/// over UDP.
///
/// The usage is `owl [OPTS] command [ARGS]` where `[OPTS]` are tool options
/// and `[ARGS]` are command arguments passed without any modification.
///
/// E.g. `owl +Host:127.0.0.1 +Port:9090 rsync -avz /home/user root@192.168.56.102:/home`.
///
/// Shell scripts can be wrapped as well with modification of shebang, e.g.
///
/// ```shell
/// #!/usr/bin/env owl +Name:Awesome_Job bash
/// ...commands...
/// ```
///
/// The tool accepts options which have form of `+Name:value` where `Name` is the name
/// of the option, case is sensitive, and `value` is the value.
///
/// Supported options:
///
/// - `Conf` is the location of the configuration file, e.g. `+Conf:/usr/local/owl.conf`.
/// - `Host` is the host address to delivert state to, e.g. `+Host:192.168.0.90`.
/// - `Port` is the port to deliver state to, e.g. `+Port:20304`.
/// - `Heartbeat` is the delay between deliveries in milliseconds, e.g. `+Heartbeat:10000`.
///
extern crate nix;
extern crate procinfo;
extern crate signal_hook;
extern crate toml;

#[macro_use]
extern crate lazy_static;

use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use procinfo::pid::{stat, Stat};
use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::net::{SocketAddr, UdpSocket};
use std::process::{self, Command};
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use std::thread;
use std::time;

// Defaults and constants
const EMPTY_STR: &str = "";
const OPTION_START: char = '+';
const OPTION_DELIMITER: char = ':';
const SECTION_WATCH: &str = "watch";
const OPT_CONF: &str = "Conf";
const OPT_HOST: &str = "Host";
const OPT_PORT: &str = "Port";
const OPT_NAME: &str = "Name";
const OPT_HEARTBEAT: &str = "Heartbeat";
const DEFAULT_REMOTE_HOST: &str = "0.0.0.0";
const DEFAULT_REMOTE_PORT: &str = "39576";
const DEFAULT_HEARTBEAT: &str = "1000";
const DEFAULT_HEARTBEAT_MILLIS: u64 = 1000;
const CONF_LOCATION_CWD: &str = "owl.toml";
const CONF_LOCATION_ETC: &str = "/etc/owl.toml";
const CONF_LOCATION_ETC_OWL: &str = "/etc/owl/owl.toml";
const UNIX_SIGNAL_EXIT_CODE: i32 = 128;
const SUCCESS: i32 = 0;

lazy_static! {
    // The id of the process which run the command.
    static ref CHILD_PID: AtomicU32 = AtomicU32::new(0);

    // The last signal caught.
    static ref LAST_SIGNAL: AtomicI32 = AtomicI32::new(0);

    // The collection of tool options.
    static ref OPT: HashMap<String, String> = collect_opts();

    // The collection of command line arguments of the command.
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

///
/// Start the command executing.
/// By default STDIN, STDOUT, and STDERR becomes standats inputs
/// and outputs for the command process.
///
fn execute_command() -> i32 {
    if let Some(name) = command_name() {
        let mut child = Command::new(name)
            .args(command_args())
            .spawn()
            .expect("failed to execute command");
        CHILD_PID.store(child.id(), Ordering::Relaxed);
        child
            .wait()
            .expect("failed to retrieve command exit status")
            .code()
            .unwrap_or(UNIX_SIGNAL_EXIT_CODE + LAST_SIGNAL.load(Ordering::Relaxed))
    } else {
        SUCCESS
    }
}

///
/// Collect the tool options from the command line and from the configuration file
/// if it exists.
/// Options passed within the command line has the biggest priority
/// and rewrite the similar options from the configuration file.
///
fn collect_opts() -> HashMap<String, String> {
    let mut dict: HashMap<String, String> = HashMap::new();

    // Collect options from command line arguments
    let opts: Vec<String> = env::args_os()
        .skip(1)
        .map(|x| x.to_string_lossy().into())
        .filter(|x: &String| x.starts_with(OPTION_START))
        .collect();

    for opt in opts {
        let mut lossy: String = opt.to_string();
        let _ = lossy.remove(0); // Strip the leading +

        // Find the index of colon and split to name and value
        let parts: Vec<&str> = lossy.splitn(2, OPTION_DELIMITER).collect();
        if parts.len() == 2 {
            dict.insert(parts[0].into(), parts[1].into());
        } else {
            dict.insert(parts[0].into(), EMPTY_STR.into());
        }
    }

    // Collect options from configuration file
    if let Some(conf) = read_config_content(dict.get(OPT_CONF)) {
        if let Some(conf) = conf.get(SECTION_WATCH) {
            if let Some(watch) = conf.as_table() {
                for entry in watch.into_iter() {
                    if dict.get(entry.0).is_none() {
                        match entry.1 {
                            toml::Value::String(v) => {
                                dict.insert(entry.0.to_string(), v.to_string())
                            }
                            toml::Value::Integer(v) => {
                                dict.insert(entry.0.to_string(), format!("{}", v))
                            }
                            toml::Value::Float(v) => {
                                dict.insert(entry.0.to_string(), format!("{}", v))
                            }
                            toml::Value::Boolean(v) => {
                                dict.insert(entry.0.to_string(), format!("{}", v))
                            }
                            toml::Value::Datetime(v) => {
                                dict.insert(entry.0.to_string(), format!("{}", v))
                            }
                            _ => None,
                        };
                    }
                }
            }
        }
    }

    dict
}

///
/// Strip all `+<Name>:<Value>` arguments and return others.
///
fn collect_command_args() -> Vec<OsString> {
    env::args_os()
        .skip(1)
        .filter(|x| !x.to_string_lossy().starts_with(OPTION_START))
        .collect()
}

///
/// Get the name of the command the child process run.
///
fn command_name() -> Option<OsString> {
    ARGS.iter().take(1).cloned().nth(0)
}

///
/// Get the list of command line arguments of the child process.
///
fn command_args() -> Vec<OsString> {
    ARGS.iter().skip(1).cloned().collect()
}

///
/// Listen for incoming OS signal in the infinite loop.
/// All signal caught are redirected as-is to the child process.
///
fn listen_signals() {
    let signals = signal_hook::iterator::Signals::new(allowed_signals())
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

///
/// Make the list of all possible allowed signals
/// which the tool can subscribe for.
///
fn allowed_signals() -> Vec<i32> {
    let all = vec![
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
    ];

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

///
/// Convert signal from the numeric representation `from` into `Signal` type
/// if possible.
///
fn cast_signal(from: i32) -> Option<Signal> {
    if let Ok(sig) = Signal::from_c_int(from) {
        Some(sig)
    } else {
        None
    }
}

///
/// Deliver process stats periodically in the infinite loop.
///
fn deliver_state() {
    // Read delivery configuration and use defaults on missing options.
    let mut remote_host = OPT.get(OPT_HOST).unwrap_or(&EMPTY_STR.to_owned()).clone();
    if remote_host.is_empty() {
        remote_host = DEFAULT_REMOTE_HOST.to_owned();
    }

    let mut remote_port = OPT.get(OPT_PORT).unwrap_or(&EMPTY_STR.to_owned()).clone();
    if remote_port.is_empty() {
        remote_port = DEFAULT_REMOTE_PORT.to_owned();
    }

    let delay = OPT
        .get(OPT_HEARTBEAT)
        .unwrap_or(&DEFAULT_HEARTBEAT.to_owned())
        .parse::<u64>()
        .unwrap_or(DEFAULT_HEARTBEAT_MILLIS);

    let remote_addr = format!("{}:{}", remote_host, remote_port);

    // Start sending notifications periodically when child PID is defined
    loop {
        let pid = CHILD_PID.load(Ordering::Relaxed);
        if pid > 0 {
            if let Some(info) = read_process_info(pid) {
                send_state(remote_addr.clone(), info);
            }

            // Sleep a little before the next delivery
            thread::sleep(time::Duration::from_millis(delay));
        }
    }
}

///
/// Send the stat of the process to the remote listener.
/// The send is done over UDP socket which is created with a random
/// port.
///
fn send_state(remote_addr: String, stat: Stat) {
    // Make temp UDP socket with OS assigned port and send message
    let local_addr = SocketAddr::from(([0, 0, 0, 0], 0));
    if let Ok(socket) = UdpSocket::bind(&local_addr) {
        // Get command name from option or from command line
        let cmd_name: String = if let Some(v) = OPT.get(OPT_NAME) {
            v.clone()
        } else {
            stat.command
        };

        let msg = format!(
            "{}||{}||{}||{:?}",
            process::id(),
            stat.pid,
            cmd_name,
            stat.state
        );
        let _ = socket.send_to(msg.as_ref(), remote_addr);
    }
}

///
/// Read stats of the process with `id` using `procinfo` crate.
/// On success stats returned or `None` otherwise.
///
fn read_process_info(id: u32) -> Option<Stat> {
    match stat(id as i32) {
        Ok(info) => Some(info),
        Err(_) => None,
    }
}

///
/// Read content of the file with `path` given. If the read is successful
/// the content is treated as toml and parsed. On any error, from reading to parsing,
/// `None` is returned.
///
fn read_file_contents<S: Into<String>>(path: S) -> Option<toml::Value> {
    match fs::File::open(path.into()) {
        Ok(mut file) => {
            let mut contents = String::new();
            if file.read_to_string(&mut contents).is_ok() {
                match contents.parse::<toml::Value>() {
                    Ok(value) => Some(value),
                    Err(_) => None,
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

///
/// Search for the configuration file and read it.
/// If `explicit_path` is given then only that file is tried to be read.
/// Otherwise the configuration file is searched in known locations.
///
fn read_config_content(explicit_path: Option<&String>) -> Option<toml::Value> {
    if let Some(path) = explicit_path {
        read_file_contents(path)
    } else {
        read_file_contents(CONF_LOCATION_CWD)
            .or_else(|| read_file_contents(CONF_LOCATION_ETC_OWL))
            .or_else(|| read_file_contents(CONF_LOCATION_ETC))
    }
}
