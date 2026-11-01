# Viernes systemd User Service Guide

Viernes can run as a background daemon managed by systemd in user space (`systemd --user`).

## Service Configuration

The service unit file `viernes.service` contains:
- **Type**: `simple`
- **Restart**: `on-failure` with `5s` restart delay
- **Isolated execution**: Runs via `uv run python -m viernes.main`
- **Environment**: Retains audio and display session variables (`DISPLAY`, `WAYLAND_DISPLAY`, `XDG_RUNTIME_DIR`, `PULSE_SERVER`).

## Installation

Run the automated installer script:
```bash
./scripts/install_service.sh
```

This creates a symlink at `~/.config/systemd/user/viernes.service` and executes `systemctl --user daemon-reload`.

## Service Management Commands

Start the service:
```bash
systemctl --user start viernes.service
```

Stop the service:
```bash
systemctl --user stop viernes.service
```

Restart the service:
```bash
systemctl --user restart viernes.service
```

Check status and PID:
```bash
systemctl --user status viernes.service
```

Enable auto-start on user session login:
```bash
systemctl --user enable viernes.service
```

Disable auto-start:
```bash
systemctl --user disable viernes.service
```

## Logs and Monitoring

Follow live log output:
```bash
journalctl --user -u viernes.service -f
```

View all logs from the current boot:
```bash
journalctl --user -u viernes.service -b
```

View the last 50 log lines:
```bash
journalctl --user -u viernes.service -n 50 --no-pager
```
