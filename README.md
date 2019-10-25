# owl
OWL is the process status notifier

## Overview

The tool wraps command in child process and sends it's state periodically
over UDP.

The usage is `owl [OPTS] command [ARGS]` where `[OPTS]` are tool options
and `[ARGS]` are command arguments passed without any modification.

E.g. `owl +Host:127.0.0.1 +Port:9090 rsync -avz /home/user root@192.168.56.102:/home` .

Shell scripts can be wrapped as well with modification of shebang, e.g.

``` shell
#!/usr/bin/env owl +Name:Awesome_Job bash
...commands...
```

The tool accepts options which have form of `+Name:value` where `Name` is the name
of the option, case is sensitive, and `value` is the value.

## Supported Options

* `Conf` is the location of the configuration file, e.g. `+Conf:/usr/local/owl.conf` .
* `Host` is the host address to delivert state to, e.g. `+Host:192.168.0.90` .
* `Port` is the port to deliver state to, e.g. `+Port:20304` .
* `Delay` is the delay between deliveries in milliseconds, e.g. `+Delay:10000` .

