# reverse_proxy

A simple reverse proxy written in Rust.

## Usage

See `reverse_proxy --help`

### Command line

For easy use on the command line, parameters are provided
- `-l` `--listen-addr`, Listen address (format 'ip:port' or 'port')
- `-r` `--remote-addr`, Upstream conn remote address (format 'ip:port')
- `--local-addr`, Upstream conn local address (format 'ip:port' or 'ip')

Here is an example of how to listen on port 10022 and forward it to 127.0.0.1:22
```shell
$ reverse_proxy --listen-addr 0.0.0.0:10022 --remote-addr 127.0.0.1:22
```

Or not specify ip (use ipv6 unspecified address).
```shell
$ reverse_proxy --listen-addr 10022 --remote-addr 127.0.0.1:22
```

If your nic has two ip such as *192.168.10.10* and *192.168.10.11*, then you can use `--local-addr` specify *192.168.10.10* as conn source address.
```shell
$ reverse_proxy --listen-addr 10022 --remote-addr 192.168.11.100:22 --local-addr 192.168.10.10
# Or
$ reverse_proxy --listen-addr 10022 --remote-addr 192.168.11.100:22 --local-addr 192.168.10.10:0
```

You can see the logs displayed on the terminal
```log
[2024-07-18T09:28:58.208Z INFO ] === Reverse Proxy start ===
[2024-07-18T09:28:58.208Z INFO ] Listen on 0.0.0.0:10022
```

### Daemon service

Daemon not have command line args `--listen-addr` and `--remote-addr`, all proxy address pair list is obtained from the config file.

#### config

You can specify the path of the config file path through the `--config` args, default path is **/etc/reverse_proxy/config.json**.

Here is an example of config content.
```json
{
    "addr_pair_list": [
        {
            "listen_addr": "0.0.0.0:10022",
            "remote_addr": "127.0.0.1:22"
        }
    ]
}
```

Provided an additional option `--dump-config`, if value set
- *env* , dump the in use config
- *default*, dump the template config

#### log

The default log path is **/var/log/reverse_proxy.log** which you can set through `--log-path`.

#### systemd

*reverse_proxy* binary should copied to **/usr/local/bin/reverse_proxy** after build.

Here is an systemd service
```shell
[Unit]
Description=Reverse Proxy Server

[Service]
Type=simple
WorkingDirectory=
ExecStart=/usr/local/bin/reverse_proxy --config /etc/reverse_proxy/config.json
Restart=always
RestartSec=3s
KillMode=process

[Install]
WantedBy=multi-user.target
```

Write it into **/usr/lib/systemd/system/reverse_proxy.service** and exec command
```shell
$ systemctl daemon-reload
$ systemctl enable reverse_proxy
$ systemctl start reverse_proxy
```

## TODO
- Support specify `local-addr` for upstream conn