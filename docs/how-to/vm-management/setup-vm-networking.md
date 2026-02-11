# Setup VM Networking

**Goal**: Configure network access for VMs booted from Strata snapshots.

## Prerequisites

- Strata CLI installed
- VM snapshot ready
- Basic understanding of networking concepts

## Networking Modes

Strata supports QEMU user-mode networking (no root required).

## Enable Basic Networking

```bash
strata vm boot vm-snapshot.st --net
```

**What this provides**:
- VM can access internet
- VM can access host via 10.0.2.2
- Host cannot directly access VM (use port forwarding)

## Port Forwarding

Forward host ports to VM services.

### Forward Single Port

```bash
# Forward host:2222 to VM:22 (SSH)
strata vm boot vm-snapshot.st \\
  --net \\
  --forward 2222:22
```

Access from host:
```bash
ssh -p 2222 user@localhost
```

### Forward Multiple Ports

```bash
strata vm boot vm-snapshot.st \\
  --net \\
  --forward 2222:22 \\    # SSH
  --forward 8080:80 \\    # HTTP
  --forward 8443:443      # HTTPS
```

### Common Port Mappings

| Service | VM Port | Host Port | Command |
|---------|---------|-----------|---------|
| SSH | 22 | 2222 | `--forward 2222:22` |
| HTTP | 80 | 8080 | `--forward 8080:80` |
| HTTPS | 443 | 8443 | `--forward 8443:443` |
| PostgreSQL | 5432 | 5432 | `--forward 5432:5432` |
| MySQL | 3306 | 3306 | `--forward 3306:3306` |
| Redis | 6379 | 6379 | `--forward 6379:6379` |

## SSH Access Configuration

### Step 1: Ensure SSH Server in VM

Inside VM:
```bash
sudo apt install openssh-server
sudo systemctl enable ssh
sudo systemctl start ssh
```

### Step 2: Boot with SSH Port Forward

```bash
strata vm boot vm-snapshot.st \\
  --net \\
  --forward 2222:22
```

### Step 3: Connect from Host

```bash
ssh -p 2222 user@localhost
```

**For key-based auth**:
```bash
# Copy SSH key to VM
ssh-copy-id -p 2222 user@localhost

# Connect without password
ssh -p 2222 user@localhost
```

### Step 4: Add SSH Config (Optional)

Edit `~/.ssh/config`:
```
Host strata-vm
    HostName localhost
    Port 2222
    User myuser
    IdentityFile ~/.ssh/id_rsa
```

Then connect with:
```bash
ssh strata-vm
```

## Web Server Access

### Run Web Server in VM

Inside VM:
```bash
# Install nginx
sudo apt install nginx
sudo systemctl start nginx
```

### Boot with HTTP Port Forward

```bash
strata vm boot vm-snapshot.st \\
  --net \\
  --forward 8080:80
```

### Access from Host

```bash
curl http://localhost:8080
# or open in browser: http://localhost:8080
```

## Database Access

### PostgreSQL Example

Inside VM:
```bash
sudo apt install postgresql
sudo systemctl start postgresql

# Configure to listen on all interfaces
sudo -u postgres psql -c "ALTER SYSTEM SET listen_addresses = '*';"
sudo systemctl restart postgresql
```

Edit `/etc/postgresql/*/main/pg_hba.conf`:
```
host    all             all             0.0.0.0/0               md5
```

Boot with port forward:
```bash
strata vm boot vm-snapshot.st \\
  --net \\
  --forward 5432:5432
```

Connect from host:
```bash
psql -h localhost -p 5432 -U postgres
```

## Development Workflow

### Full Development Environment

```bash
strata vm boot dev-vm.st \\
  --ram 8G \\
  --cpus 4 \\
  --net \\
  --forward 2222:22 \\     # SSH
  --forward 8080:8080 \\   # Dev server
  --forward 5432:5432 \\   # PostgreSQL
  --forward 6379:6379      # Redis
```

Access services:
```bash
# SSH
ssh -p 2222 dev@localhost

# Dev server
curl http://localhost:8080

# Database
psql -h localhost -p 5432 -U dev
```

## File Transfer

### Using SCP

```bash
# Copy file to VM
scp -P 2222 localfile.txt user@localhost:/remote/path/

# Copy file from VM
scp -P 2222 user@localhost:/remote/file.txt ./local/path/
```

### Using SFTP

```bash
sftp -P 2222 user@localhost
sftp> put localfile.txt
sftp> get remotefile.txt
sftp> quit
```

### Using rsync

```bash
# Sync directory to VM
rsync -avz -e "ssh -p 2222" ./local/dir/ user@localhost:/remote/dir/

# Sync from VM
rsync -avz -e "ssh -p 2222" user@localhost:/remote/dir/ ./local/dir/
```

## Network Performance

### Measure Throughput

Inside VM:
```bash
# Install iperf3
sudo apt install iperf3

# Run server
iperf3 -s
```

On host:
```bash
# Install iperf3
sudo apt install iperf3

# Test (after port forwarding 5201:5201)
iperf3 -c localhost -p 5201
```

Expected performance: 1-10 Gbps depending on CPU

## Troubleshooting

### Cannot Connect to Forwarded Port

**Check VM service is running**:
```bash
# Inside VM
sudo netstat -tlnp | grep :80  # Check if service listening
```

**Check firewall**:
```bash
# Inside VM
sudo ufw status
sudo ufw allow 80/tcp
```

**Verify port forward syntax**:
```bash
# Correct
--forward 8080:80  # host:vm

# Incorrect
--forward 80:8080  # backwards
```

### SSH Connection Refused

**Ensure SSH server running in VM**:
```bash
sudo systemctl status ssh
sudo systemctl start ssh
```

**Check SSH listening on all interfaces**:
```bash
# Inside VM
sudo nano /etc/ssh/sshd_config
# Ensure: ListenAddress 0.0.0.0
sudo systemctl restart ssh
```

### Slow Network Performance

- Ensure KVM enabled (not QEMU emulation)
- Increase VM CPUs: `--cpus 4`
- Check host network not saturated

## Advanced: Custom Network Configuration

For more complex networking (bridged, TAP), see QEMU networking documentation. Strata CLI focuses on simple user-mode networking for most use cases.

## See Also

- [How-To: Create VM Snapshots](create-vm-snapshots.md)
- [How-To: Boot VM from Snapshot](boot-vm-from-snapshot.md)
- [Tutorial: Booting Your First VM](../../tutorials/booting-your-first-vm.md)
- [Reference: CLI Commands](../../reference/cli-reference.md)
