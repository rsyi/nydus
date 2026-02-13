# Nydus Implementation Notes

## Project Overview

Nydus is a local-first Rust CLI + TUI tool for managing EC2 dev environments with built-in SSH tunneling, browser integration, and credential synchronization.

**Status**: Phases 1-3 Complete (Fully Functional CLI)

## Implementation Phases

### Phase 1: Project Scaffolding ✅

**Completed**: Core infrastructure and data models

- Cargo project with all dependencies (AWS SDK, Ratatui, SQLite, etc.)
- Module structure created
- Core types defined:
  - `Profile` - EC2 instance configuration profiles
  - `Instance` - Running/stopped EC2 instance state
  - `Tunnel` - SSH port forward tracking
- Config management with YAML serialization
- SQLite database with tables:
  - `instances` - Instance metadata and state
  - `tunnels` - Active SSH tunnels with PIDs
  - `context` - Current active instance
- Custom error types with `thiserror`
- `nydus init` command working

**Key Files**:
- `src/config.rs` - Profile and config YAML management
- `src/state.rs` - SQLite CRUD operations
- `src/error.rs` - NydusError enum

### Phase 2: AWS Integration ✅

**Completed**: Full EC2 lifecycle management

- AWS EC2 client initialization with credential chain
- Instance creation with automatic Ubuntu AMI resolution
- Instance lifecycle: start, stop, terminate
- Security group auto-creation with SSH access from your IP
- Instance import from existing EC2 instances
- Context switching between instances

**Key Files**:
- `src/aws/ec2.rs` - All EC2 operations

**Notable Implementation Details**:
- Uses `reqwest` to fetch public IP from ipify.org
- Resolves latest Ubuntu 22.04 LTS AMI automatically
- Creates per-instance security groups (`nydus-{instance_name}`)
- Wait-for-running logic with visual progress dots

### Phase 3: SSH, Tunneling & Credential Sync ✅

**Completed**: Full SSH integration and credential propagation

**SSH Features**:
- Interactive SSH sessions using system `ssh` binary
- Foreground tunnels (blocking)
- Background tunnels with PID tracking
- Tunnel lifecycle management (start/stop)
- Auto port allocation for tunnels
- Browser integration for tunneled ports

**Credential Sync Features**:
- Git SSH keys with proper 600 permissions
- Git config (~/.gitconfig)
- GPG keys (partial - needs improvement)
- Claude session tokens and API keys
- AWS credentials (~/.aws/credentials, ~/.aws/config)
- Environment variables with ${VAR} interpolation
- Arbitrary dotfiles (~/.vimrc, ~/.tmux.conf, etc.)
- SSH agent setup on remote with key loading

**Key Files**:
- `src/ssh.rs` - SSH operations and tunneling
- `src/sync.rs` - Credential propagation logic

**Security Model**:
- Uses system `scp` for file transfer
- SSH keys copied with mode 600
- Credentials transferred over encrypted SSH connection
- Keys never logged or persisted in nydus state
- Security groups auto-restrict to current public IP

### Phase 4: TUI ⏳ (Not Yet Implemented)

**Planned**: Interactive terminal UI with ratatui

- Event loop with tokio::select!
- Non-blocking AWS operations via message channels
- Keyboard shortcuts for all operations
- Real-time status updates
- Visual tunnel monitoring with alive/dead PIDs

### Phase 5: Polish ⏳ (Not Yet Implemented)

**Planned**: Production readiness

- Logging with tracing crate
- Comprehensive error messages
- Cross-platform testing (macOS, Linux, Windows)
- Integration tests with test AWS environment
- Performance optimizations
- Additional profile management features

## Technical Architecture

### Dependencies

```toml
clap 4.5          # CLI argument parsing
ratatui 0.28      # TUI framework (ready for Phase 4)
crossterm 0.28    # Terminal manipulation
aws-sdk-ec2 1.82  # AWS EC2 API
aws-config 1.5    # AWS credential loading
rusqlite 0.32     # Embedded SQLite database
serde 1.0         # Serialization
serde_yaml 0.9    # YAML config format
tokio 1.40        # Async runtime
anyhow 1.0        # CLI error handling
thiserror 1.0     # Library error types
tracing 0.1       # Logging framework
chrono 0.4        # Date/time utilities
home 0.5          # Cross-platform home directory
reqwest 0.12      # HTTP client for public IP lookup
```

### Database Schema

```sql
-- Instance tracking
CREATE TABLE instances (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    profile TEXT NOT NULL,
    region TEXT NOT NULL,
    instance_id TEXT NOT NULL,
    public_ip TEXT,
    public_dns TEXT,
    private_ip TEXT,
    ssh_user TEXT NOT NULL,
    ssh_key_path TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_seen INTEGER NOT NULL,
    last_synced INTEGER,
    desired_state TEXT NOT NULL,
    status TEXT,
    notes TEXT
);

-- SSH tunnel tracking
CREATE TABLE tunnels (
    id INTEGER PRIMARY KEY,
    instance_name TEXT NOT NULL,
    remote_host TEXT NOT NULL DEFAULT '127.0.0.1',
    remote_port INTEGER NOT NULL,
    local_port INTEGER NOT NULL,
    pid INTEGER,
    started_at INTEGER NOT NULL,
    mode TEXT NOT NULL,
    open_url INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (instance_name) REFERENCES instances(name) ON DELETE CASCADE
);

-- Context management
CREATE TABLE context (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

### Key Design Decisions

1. **SQLite over JSON**: Enables structured queries, ACID properties, easier feature expansion
2. **System SSH over library**: Better compatibility, easier debugging, handles known_hosts automatically
3. **SSH-based credential sync**: More secure than EC2 user_data (which is visible in AWS Console)
4. **YAML config**: Human-readable, version-controllable, team-shareable
5. **Tokio async**: Required for AWS SDK, prepares for TUI event loop
6. **Profile-based config**: Multiple environment templates (dev, prod, etc.)

## CLI Commands Reference

### Instance Management
```bash
nydus init                           # Initialize ~/.nydus/ directory
nydus up <name> --profile <profile>  # Create & start instance
nydus down [name]                    # Stop instance (uses context if no name)
nydus terminate <name> [--yes]       # Terminate instance
nydus ls                             # List all instances with status
nydus import --instance-id <id> --name <name> --region <region>
             --ssh-user <user> --key <path> --profile <profile>
nydus switch <name>                  # Set current context
```

### SSH & Tunneling
```bash
nydus attach [name]                  # SSH into instance
nydus forward [name] --remote <port> [--local <port>] [--background] [--open]
nydus open [name] --remote <port>    # Open browser to tunneled port
nydus tunnels [name]                 # List active tunnels
nydus tunnel-stop <id>               # Stop specific tunnel
nydus sync [name]                    # Sync credentials manually
```

### Profile Management
```bash
nydus profile list                   # List all profiles
nydus profile show <name>            # Show profile details
nydus profile add <name>             # Add new profile (creates template)
```

### TUI (Phase 4)
```bash
nydus                                # Launch interactive TUI
nydus tui                            # Same as above
```

## Configuration

Located at `~/.nydus/config.yaml`

```yaml
profiles:
  - name: dev
    region: us-east-1
    instance_type: t3.medium
    ami: null  # Auto-resolves to latest Ubuntu 22.04 LTS
    ssh_user: ubuntu
    ssh_key_path: ~/.ssh/id_ed25519
    security_group: null  # Auto-creates
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
        config: true
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

## Known Limitations & Future Work

### Current Limitations
1. GPG key sync not fully implemented (needs proper export/import)
2. No TUI yet (Phase 4)
3. SSH uses `-o StrictHostKeyChecking=no` (convenience vs security trade-off)
4. No support for non-default VPCs (uses default subnet)
5. Windows support not tested

### Future Enhancements
1. Implement TUI with real-time status updates
2. Add instance tagging and filtering in `ls` command
3. Support for instance resizing
4. Snapshot/backup management
5. Multi-region instance management
6. Team sharing of profiles (git-based?)
7. Cost tracking and estimates
8. Auto-stop after inactivity
9. Integration with IDE remote development features
10. Support for other cloud providers (GCP, Azure)

## Testing Notes

### Manual Testing Checklist
- [x] `nydus init` creates directory and files
- [ ] `nydus up` creates EC2 instance (requires AWS credentials)
- [ ] `nydus ls` shows instances
- [ ] `nydus attach` opens SSH session
- [ ] `nydus forward` creates tunnel
- [ ] `nydus sync` copies credentials
- [ ] Context switching works
- [ ] Tunnel PID tracking accurate

### AWS Permissions Required

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

## Build Information

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Run without installing
cargo run -- <command>

# Install globally
cargo install --path .

# Check compilation
cargo check
```

Binary size: ~3.9MB (release mode)

## Code Statistics

- Total lines: ~2500
- Modules: 11
- CLI commands: 14
- Database tables: 3
- Error variants: 8
- Tests: None yet (TODO)

## Troubleshooting

### Common Issues

**Issue**: "No AWS credentials found"
- **Solution**: Configure AWS CLI or set environment variables

**Issue**: "Permission denied" when SSHing
- **Solution**: Ensure SSH key has correct permissions (600)

**Issue**: "Security group already exists"
- **Solution**: Use different instance name or manually delete old security group

**Issue**: Tunnel PID shows as "stopped" but port still in use
- **Solution**: Manually kill the process or use a different local port

## Contributing Guidelines

1. Follow existing code style
2. Add error handling for all operations
3. Update README when adding features
4. Test with real AWS resources before committing
5. Never commit AWS credentials or personal instance data

## License

MIT
