CLI app that currently prints mac addresses of arp requests on provided interface.

- run with:
```
sudo -E capsh --caps="cap_setpcap,cap_setuid,cap_setgid+ep cap_sys_ptrace,cap_net_raw,cap_net_admin+eip" --keep=1 --user="$USER" --addamb="cap_sys_ptrace,cap_net_raw,cap_net_admin" --shell=$(which cargo) -- run --example cli -- --interface <interface>
```
