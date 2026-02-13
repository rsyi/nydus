# Nydus

A local-first Rust CLI + TUI tool for managing EC2 dev environments with built-in SSH tunneling, browser integration, and credential synchronization.

## Features

- **Simple Instance Management**: Create, start, stop, and terminate EC2 instances with a single command
- **Local State Tracking**: SQLite database maintains instance state locally
- **SSH Integration**: Built-in SSH connection and port forwarding
- **Credential Sync**: Automatically propagate git, Claude, and AWS credentials to remote instances
- **TUI Interface**: Interactive terminal UI for managing multiple instances
- **Context Switching**: Quickly switch between different dev environments

## Installation

```bash
cargo install --path .
```

## Quick Start

1. **Initialize nydus**:
   ```bash
   nydus init
   ```

2. **Configure a profile** (edit `~/.nydus/config.yaml`):
   ```yaml
   profiles:
     - name: dev
       region: us-east-1
       instance_type: t3.medium
       ssh_user: ubuntu
       ssh_key_path: ~/.ssh/id_ed25519
       volume_size_gb: 20
       sync_credentials:
         enabled: true
         git:
           ssh_keys:
             - ~/.ssh/id_ed25519
           config: true
   ```

3. **Create an instance**:
   ```bash
   nydus up mydev --profile dev
   ```

4. **Connect to it**:
   ```bash
   nydus attach mydev
   ```

## Commands

### Instance Management

- `nydus init` - Initialize configuration directory
- `nydus up <name> --profile <profile>` - Create and start a new instance
- `nydus down [name]` - Stop an instance
- `nydus terminate <name>` - Terminate an instance
- `nydus ls` - List all instances
- `nydus import` - Import an existing EC2 instance

### SSH & Tunneling

- `nydus attach [name]` - SSH into an instance
- `nydus forward [name] --remote <port> --local <port>` - Create SSH tunnel
- `nydus open [name] --remote <port>` - Open browser to tunneled port
- `nydus tunnels [name]` - List active tunnels

### Context Management

- `nydus switch <name>` - Set current context to an instance
- `nydus sync [name]` - Sync credentials to instance

### Profile Management

- `nydus profile list` - List all profiles
- `nydus profile show <name>` - Show profile details

### Interactive TUI

- `nydus` or `nydus tui` - Launch interactive TUI

## Configuration

Configuration is stored in `~/.nydus/config.yaml`.

### Profile Schema

```yaml
profiles:
  - name: dev
    region: us-east-1
    instance_type: t3.medium
    ami: null  # Will use latest Ubuntu 22.04 LTS
    ssh_user: ubuntu
    ssh_key_path: ~/.ssh/id_ed25519
    security_group: null  # Will create automatically
    subnet_id: null
    volume_size_gb: 20
    tags:
      Environment: development
      Project: myproject
    sync_credentials:
      enabled: true
      git:
        ssh_keys:
          - ~/.ssh/id_ed25519
        config: true  # Copy ~/.gitconfig
        gpg_keys: false
      claude:
        enabled: true
        session_file: ~/.claude/session_token
        api_key_env: ANTHROPIC_API_KEY
      aws:
        enabled: false
      env_vars:
        NODE_ENV: development
        EDITOR: vim
      dotfiles:
        - ~/.vimrc
        - ~/.tmux.conf
```

## AWS Permissions

Nydus requires the following IAM permissions:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": [
        "ec2:RunInstances",
        "ec2:StartInstances",
        "ec2:StopInstances",
        "ec2:TerminateInstances",
        "ec2:DescribeInstances",
        "ec2:DescribeImages",
        "ec2:DescribeSecurityGroups",
        "ec2:CreateSecurityGroup",
        "ec2:AuthorizeSecurityGroupIngress",
        "ec2:CreateTags",
        "ec2:DescribeSubnets",
        "ec2:DescribeVpcs"
      ],
      "Resource": "*"
    }
  ]
}
```

## Development Status

### ✅ Phase 1: Project Scaffolding & Core Types (Completed)
- Cargo project structure
- Core types (Profile, Instance, Tunnel)
- Config YAML loading/saving
- SQLite state database with CRUD operations
- `nydus init` command

### ✅ Phase 2: AWS Integration (Completed)
- EC2 client initialization
- Instance lifecycle operations (run, start, stop, terminate)
- AMI resolution (latest Ubuntu LTS)
- Security group management
- CLI commands: up, down, terminate, ls, import, switch

### ✅ Phase 3: SSH, Tunneling & Credential Sync (Completed)
- SSH attach functionality
- Port forwarding (foreground/background modes)
- Background tunnel PID tracking
- Credential synchronization (git, Claude, AWS, env vars, dotfiles)
- Browser integration
- CLI commands: attach, forward, open, sync, tunnels, tunnel-stop
- Profile management commands

### 📋 Phase 4: TUI (Planned)
- Interactive terminal UI with ratatui
- Non-blocking AWS operations
- Keybinds for all operations

### 📋 Phase 5: Polish & Documentation (Planned)
- Additional profile management commands
- Logging with tracing
- Comprehensive testing
- Cross-platform support

## License

MIT

## Contributing

Contributions welcome! Please open an issue or PR.
