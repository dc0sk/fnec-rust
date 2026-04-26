---
title: T480 Validation Host Setup
status: draft
last-updated: 2026-04-26
---

# T480 CUDA & Intel GPU Validation Host

## Hardware Assessment

| Component | Model | CUDA / Compute | Notes |
|---|---|---|---|
| NVIDIA GPU | GeForce MX150 | CUDA CC 6.1 (Pascal, GP108) | ✅ Sufficient for correctness + benchmarking |
| Intel iGPU | UHD Graphics 620 | OpenCL 3.0 (Gen 9.5) | ✅ OpenCL validation; no Xe/Arc, no Level Zero |
| CPU | Intel Core i5/i7-8xxx | — | Baseline CPU reference runs |

### Is the MX150 sufficient?

Yes, for this project's goals:

- CUDA CC 6.1 supports all standard CUDA compute features used (device memory, kernel launches, atomic ops).
- Clock speeds and memory bandwidth are low (~25 GB/s), so measured timings will not reflect production hardware — but they will catch correctness regressions and exercise the GPU code path end-to-end.
- The 2 GB VRAM is tight; keep problem sizes (segment counts) modest in GPU benchmarks. The reference corpus decks (≤51 segments) are fine.

### Intel UHD 620 — what it validates

- OpenCL 3.0 runtime correctness on Intel Gen 9.5
- Does **not** represent Intel Arc (Xe-HPG, XMX) or Intel oneAPI Level Zero
- Useful as a second vendor for OpenCL portability checks; not a proxy for discrete Intel GPU performance

---

## Option A — Linux (Recommended)

Linux is preferred: native CUDA toolchain, no driver signing issues, and SSH access already planned.

### 1. Install CUDA Toolkit (Debian/Ubuntu)

```bash
# Add NVIDIA package repo (adjust distro/arch as needed)
wget https://developer.download.nvidia.com/compute/cuda/repos/ubuntu2204/x86_64/cuda-keyring_1.1-1_all.deb
sudo dpkg -i cuda-keyring_1.1-1_all.deb
sudo apt update
sudo apt install -y cuda-toolkit-12-4
```

Add to `~/.bashrc` or `~/.zshrc`:

```bash
export PATH=/usr/local/cuda/bin:$PATH
export LD_LIBRARY_PATH=/usr/local/cuda/lib64:$LD_LIBRARY_PATH
```

Verify:

```bash
nvcc --version
nvidia-smi
```

### 2. Install Intel OpenCL Runtime

```bash
sudo apt install -y intel-opencl-icd ocl-icd-opencl-dev clinfo
clinfo | head -20
```

The UHD 620 should appear as an OpenCL platform. If the package is not found, use the Intel compute-runtime PPA:

```bash
sudo add-apt-repository ppa:intel-opencl/intel-opencl
sudo apt update
sudo apt install intel-opencl-icd
```

### 3. Rust GPU toolchain

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# No extra Rust toolchain changes needed for CUDA FFI path
# For opencl crate:
sudo apt install -y ocl-icd-opencl-dev
```

### 4. Build and run GPU benchmarks

```bash
cd ~/fnec-rust
cargo build --release
# Run the reference dipole with hybrid exec mode:
./target/release/fnec --solver hallen --exec hybrid corpus/dipole-freesp-51seg.nec
# Check diag line for timing breakdown
```

### 5. SSH remote access setup

On the T480 (server side):

```bash
sudo apt install -y openssh-server
sudo systemctl enable --now ssh
# Note the IP: ip addr show
```

From your workstation:

```bash
ssh user@<t480-ip>
# Or add to ~/.ssh/config:
# Host t480
#   HostName <t480-ip>
#   User <username>
#   IdentityFile ~/.ssh/id_ed25519
```

Forward VS Code Remote SSH to it via the Remote-SSH extension — no further configuration needed for editing and running tests.

---

## Option B — Windows

Windows adds CUDA driver/toolkit installation complexity and a remote desktop layer, but is useful for validating the Windows build path.

### 1. Enable OpenSSH Server (Windows 10/11)

In PowerShell (as Administrator):

```powershell
Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0
Set-Service -Name sshd -StartupType Automatic
Start-Service sshd
# Allow through firewall (usually done automatically):
New-NetFirewallRule -Name sshd -DisplayName 'OpenSSH Server' -Enabled True -Direction Inbound -Protocol TCP -Action Allow -LocalPort 22
```

Verify from your workstation:

```bash
ssh user@<t480-ip>
```

### 2. Remote Desktop (RDP) via SSH tunnel

RDP (port 3389) gives a full GUI session. Tunnel it over SSH to avoid exposing RDP directly:

**On your workstation**, open the tunnel:

```bash
ssh -L 13389:localhost:3389 user@<t480-ip> -N &
```

Then connect your RDP client to `localhost:13389`. On Linux use `remmina` or `xfreerdp`:

```bash
# Install:
sudo apt install -y freerdp2-x11
# Connect:
xfreerdp /v:localhost:13389 /u:<WindowsUsername> /p:<password> /dynamic-resolution
```

On macOS use Microsoft Remote Desktop from the App Store, pointed at `localhost:13389`.

On Windows use the built-in `mstsc.exe`, connect to `localhost:13389`.

#### Enable RDP on the T480 Windows side

Settings → System → Remote Desktop → Enable Remote Desktop. Make sure the account used has "Allow log on through Remote Desktop Services" permission (local Administrators group is sufficient).

### 3. Install CUDA on Windows

Download CUDA Toolkit from https://developer.nvidia.com/cuda-downloads (select Windows → x86_64 → exe(network)).  
Install with default options; the MX150 driver will be bundled.

Verify in a command prompt:

```cmd
nvcc --version
nvidia-smi
```

### 4. Install Intel Arc/OpenCL runtime (Windows)

For UHD 620 OpenCL on Windows, the standard Intel graphics driver includes OpenCL support — no separate install needed if the driver is up to date. Download from https://www.intel.com/content/www/us/en/download/785597/intel-arc-iris-xe-graphics-windows.html.

Verify with GPU-Z or `clinfo` (install OpenCL SDK from Intel oneAPI Base Toolkit if `clinfo` is needed).

### 5. Rust + build on Windows

```powershell
# Install rustup from https://rustup.rs (downloads rustup-init.exe)
# Default stable toolchain is fine
rustup update stable

# In the repo:
cd C:\path\to\fnec-rust
cargo build --release
.\target\release\fnec.exe --solver hallen --exec hybrid corpus\dipole-freesp-51seg.nec
```

---

## Recommendation

| Goal | Preferred OS |
|---|---|
| CUDA correctness gate | Linux |
| Intel OpenCL correctness gate | Linux (simpler driver story) |
| Windows build parity check | Windows (run once per milestone) |
| CI-like repeated automation | Linux via SSH |

Start with Linux. Reserve Windows for occasional build-parity smoke tests rather than regular CI runs.
