# owl
OWL is the process status notifier

## Overview

The tool wraps command in child process and sends it's state periodically
over UDP.

The usage is `owl [OPTS] command [ARGS]` where `[OPTS]` are tool options
and `[ARGS]` are command arguments passed without any modification.

E.g. `owl +Host:127.0.0.1 +Port:9090 rsync -avz /home/user root@192.168.56.102:/home` 

Shell scripts can be wrapped as well with modification of shebang, e.g.

``` shell
#!/usr/bin/env owl +Name:Awesome_Job bash
...commands...
```

The tool accepts options which have form of `+Name:value` where `Name` is the name
of the option, case is sensitive, and `value` is the value.

## Options

The format of options starting with plus and delimited with colon had been choosen
to have OWL options visually and physically separated from other command line arguments.

| Name | Description | Example |
| :--: | :---------- | :------ |
| `Conf` | The location of the configuration file.| `+Conf:/usr/local/owl.conf` |
| `Host` | The host address to delivert state to.| `+Host:192.168.0.90` |
| `Port` | The port to deliver state to.| `+Port:20304` |
| `Heartbeat` | The delay between deliveries in milliseconds.| `+Heartbeat:10000` |

## Configuration File

Options can be organized in a configuration file which is the text file in 
[toml](https://en.wikipedia.org/wiki/TOML) format.

The file should contain one section `[watch]` where all process watch related options
are. Option names here are the same as command line options.

Example configuration file.

``` toml
[watch]
Host = "192.168.20.19"
Port = 9090
Heartbeat = 5000
```

The location of the configuration file to load on tool start can be set explicitly with
the `Conf` option. In the case the `Conf` option omitted the configuration file is
searched through the default locations in the order as shown below.

1. `owl.toml` in the current work directory of the process.
2. `/etc/owl/owl.toml` 
3. `/etc/owl.toml` 

