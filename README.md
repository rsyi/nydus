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

### Option 1: Download Prebuilt Binary (Recommended)

**Linux x86_64:**
```bash
curl -L https://github.com/oxy-hq/nydus/releases/latest/download/nydus-linux-x86_64 -o nydus
chmod +x nydus
sudo mv nydus /usr/local/bin/
```

**Linux ARM64 (Android, ARM servers):**
```bash
curl -L https://github.com/oxy-hq/nydus/releases/latest/download/nydus-linux-aarch64 -o nydus
chmod +x nydus
# For Termux on Android:
mv nydus ~/bin/nydus
# For regular Linux:
# sudo mv nydus /usr/local/bin/
```

**macOS (Apple Silicon):**
```bash
curl -L https://github.com/oxy-hq/nydus/releases/latest/download/nydus-macos-aarch64 -o nydus
chmod +x nydus
sudo mv nydus /usr/local/bin/
```

**macOS (Intel):**
```bash
curl -L https://github.com/oxy-hq/nydus/releases/latest/download/nydus-macos-x86_64 -o nydus
chmod +x nydus
sudo mv nydus /usr/local/bin/
```

### Option 2: Build from Source

```bash
cargo install --path .
```

## Quick Start

### 1. Install nydus

```bash
cargo install --path .
# Or use the alias from your shell config
```

### 2. Initialize nydus

```bash
nydus init
```

This creates `~/.nydus/` with a default profile configured with credential sync.

### 3. Set up AWS Prerequisites

#### A. Create IAM User (if you don't have one)

1. **Log in to AWS Console**
2. Go to **IAM** → **Users** → **Create user**
3. User name: `nydus-user` (or your preferred name)
4. Click **Next**
5. **Attach policies directly** → Select **AmazonEC2FullAccess** (or use the custom policy below)
6. Click **Create user**

#### B. Create Access Keys

1. In **IAM** → **Users** → Select your user → **Security credentials**
2. Scroll to **Access keys** → **Create access key**
3. Choose **Command Line Interface (CLI)**
4. Check "I understand..." → **Next** → **Create access key**
5. **Download** or copy the:
   - Access key ID (starts with `AKIA...`)
   - Secret access key (long random string)

#### C. Configure AWS Credentials

Create or edit `~/.aws/credentials`:

```bash
mkdir -p ~/.aws
cat > ~/.aws/credentials << 'EOF'
[default]
aws_access_key_id = YOUR_ACCESS_KEY_ID
aws_secret_access_key = YOUR_SECRET_ACCESS_KEY
EOF
chmod 600 ~/.aws/credentials
```

Create or edit `~/.aws/config`:

```bash
cat > ~/.aws/config << 'EOF'
[default]
region = us-east-1
output = json
EOF
```

#### D. Create EC2 Key Pair

1. Go to **EC2** → **Key Pairs** (under Network & Security)
2. Click **Create key pair**
3. Name: `nydus-key` (or any name you prefer)
4. Key pair type: **ED25519** (recommended) or **RSA**
5. Private key file format: **.pem**
6. Click **Create key pair** → Download the .pem file

Move the key to the right location:

```bash
mv ~/Downloads/nydus-key.pem ~/.ssh/nydus-key.pem
chmod 600 ~/.ssh/nydus-key.pem
```

Update your nydus config (`~/.nydus/config.yaml`) to use this key:

```yaml
profiles:
  - name: default
    ssh_key_path: ~/.ssh/nydus-key.pem
    # ... rest of config
```

### 4. Create Your First Instance

```bash
# Profile defaults to "default" if not specified
nydus up mydev

# Or explicitly specify profile
nydus up mydev --profile default
```

### 5. Connect to It

```bash
nydus attach mydev
```

That's it! Your EC2 instance is ready with all your credentials synced.

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

### Option 1: Use AmazonEC2FullAccess (Easiest)

When creating your IAM user, attach the **AmazonEC2FullAccess** managed policy. This gives full EC2 permissions.

### Option 2: Use Custom Policy (More Restrictive)

For better security, create a custom policy with only the permissions nydus needs:

1. **IAM** → **Policies** → **Create policy** → **JSON**
2. Paste the policy below:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "NydusEC2Permissions",
      "Effect": "Allow",
      "Action": [
        "ec2:RunInstances",
        "ec2:StartInstances",
        "ec2:StopInstances",
        "ec2:TerminateInstances",
        "ec2:DescribeInstances",
        "ec2:DescribeInstanceStatus",
        "ec2:DescribeImages",
        "ec2:DescribeSecurityGroups",
        "ec2:CreateSecurityGroup",
        "ec2:AuthorizeSecurityGroupIngress",
        "ec2:CreateTags",
        "ec2:DescribeSubnets",
        "ec2:DescribeVpcs",
        "ec2:DescribeKeyPairs"
      ],
      "Resource": "*"
    }
  ]
}
```

3. Name it: `NydusPolicy`
4. Attach it to your IAM user

**Note**: The `"Resource": "*"` is required for EC2 instance operations as you don't know instance IDs before creation.

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
