# RustOS Installation Guide

This guide covers installing RustOS v0.1.0 on various platforms.

## Quick Start

### QEMU (Recommended for Testing)
```bash
# Download ISO
wget https://releases.rustos.org/v0.1.0/rustos-v0.1.0-x86_64.iso

# Create disk image
qemu-img create -f raw rustos.img 2G

# Boot installer
qemu-system-x86_64 \
  -cdrom rustos-v0.1.0-x86_64.iso \
  -drive file=rustos.img,format=raw \
  -m 1G \
  -smp 2 \
  -boot d \
  -netdev user,id=net0 \
  -device virtio-net-pci,netdev=net0
```

## Installation Methods

### Method 1: Interactive Installer

1. Boot from ISO
2. Select "Install RustOS" from boot menu
3. Choose target disk
4. Configure partitioning (automatic or manual)
5. Set root password
6. Wait for installation to complete
7. Reboot and remove ISO

### Method 2: Manual Installation

#### Partitioning
```bash
# In live environment
fdisk /dev/sda

# Create partitions:
# - /dev/sda1: 512M EFI System Partition (ESP)
# - /dev/sda2: Remainder for root filesystem

# Format partitions
mkfs.fat -F32 /dev/sda1
mkfs.ext4 /dev/sda2

# Mount
mount /dev/sda2 /mnt
mkdir -p /mnt/boot/efi
mount /dev/sda1 /mnt/boot/efi
```

#### Install System
```bash
# Download and extract
wget https://releases.rustos.org/v0.1.0/rustos-v0.1.0-source.tar.gz
tar xzf rustos-v0.1.0-source.tar.gz -C /mnt

# Install bootloader (from live environment)
grub-install --target=x86_64-efi --efi-directory=/mnt/boot/efi --bootloader-id=RustOS

# Generate fstab
genfstab -U /mnt >> /mnt/etc/fstab
```

#### First Boot Configuration
```bash
# Chroot into new system
arch-chroot /mnt

# Set hostname
echo "rustos" > /etc/hostname

# Create users
useradd -m -G wheel admin
passwd admin

# Exit chroot and reboot
exit
reboot
```

### Method 3: Network Installation (PXE)

#### Server Setup
```bash
# Install required packages
apt install dnsmasq syslinux

# Configure dnsmasq
cat > /etc/dnsmasq.d/rustos.conf << EOF
interface=eth0
dhcp-range=192.168.1.100,192.168.1.200,12h
dhcp-boot=pxelinux.0
enable-tftp
tftp-root=/tftpboot
EOF

# Prepare files
mkdir -p /tftpboot
cp /usr/lib/PXELINUX/pxelinux.0 /tftpboot/
cp rustos-v0.1.0/kernel.bin /tftpboot/
cp rustos-v0.1.0/initrd.img /tftpboot/

# PXE config
mkdir -p /tftpboot/pxelinux.cfg
cat > /tftpboot/pxelinux.cfg/default << EOF
DEFAULT rustos
LABEL rustos
    KERNEL kernel.bin
    APPEND initrd=initrd.img quiet
EOF

# Start dnsmasq
systemctl restart dnsmasq
```

#### Client Boot
1. Enable PXE boot in BIOS/UEFI
2. Client will receive IP and boot files automatically

### Method 4: Build from Source

#### Prerequisites
```bash
# Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default nightly
rustup component add rust-src

# Build tools
sudo apt install build-essential qemu-system-x86 nasm mtools
cargo install cargo-xbuild cargo-binutils
```

#### Build
```bash
git clone https://github.com/rust-os/rustos.git
cd rustos
git checkout v0.1.0

# Build kernel
make build

# Build ISO
make iso

# Test in QEMU
make run
```

## Post-Installation

### Update System
```bash
rustos-update
```

### Configure Network
```bash
# Static IP
cat > /etc/network/interfaces << EOF
auto eth0
iface eth0 inet static
    address 192.168.1.100
    netmask 255.255.255.0
    gateway 192.168.1.1
    dns-nameservers 8.8.8.8 8.8.4.4
EOF

# Or use DHCP
dhclient eth0
```

### Enable Services
```bash
# SSH server
systemctl enable sshd
systemctl start sshd

# Network time
systemctl enable ntpd
systemctl start ntpd
```

## Troubleshooting

### Boot Issues

**Problem:** Kernel panics on boot  
**Solution:** Add `nomodeset` to kernel parameters

**Problem:** No display output  
**Solution:** Try `video=1024x768` or check GPU compatibility

**Problem:** Disk not detected  
**Solution:** Ensure AHCI mode is enabled in BIOS (not IDE/RAID)

### Installation Failures

**Problem:** "No space left on device"  
**Solution:** Ensure target disk has at least 512MB free

**Problem:** GRUB installation fails  
**Solution:** Try `--force` flag or use alternative bootloader (systemd-boot)

### Network Issues

**Problem:** No network connectivity  
**Solution:** 
1. Check virtio drivers are loaded
2. Verify DHCP server is running
3. Try different network model: `-device e1000,netdev=net0`

## Verification

After installation, verify:
```bash
# Check kernel version
uname -r  # Should show 0.1.0

# Check CPU cores
nproc

# Check memory
free -h

# Test network
ping -c 4 8.8.8.8

# Test filesystem
df -h
```

## Uninstallation

To remove RustOS:
1. Boot from another OS
2. Delete RustOS partitions using fdisk/gparted
3. Remove bootloader entry:
   ```bash
   # For GRUB
   grub-mkconfig -o /boot/grub/grub.cfg
   
   # For systemd-boot
   rm /boot/loader/entries/rustos.conf
   ```

## Support

For additional help:
- Documentation: https://docs.rustos.org/install
- Forum: https://forum.rustos.org
- IRC: #rustos on Libera.Chat
