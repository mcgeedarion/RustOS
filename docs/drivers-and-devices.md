# Drivers and devices

The driver tree includes kernel drivers, shared bus/transport support, device schemes, and prototype userspace-driver interfaces. This is a source catalogue for `08a14f6`, not a hardware compatibility list or a claim that every device can initialize successfully.

## Enumeration and userspace access

- **PCI**: `device/pci/mod.rs` provides configuration-space enumeration and helpers, with `msix.rs` for MSI-X. The additional `device/pci/enumerate.rs` file is not declared by the PCI root.
- **Device filesystem**: `fs/devfs.rs` supplies the kernel's device-filesystem provider. It is not a submodule of the bus manager.
- **Userspace drivers**: `syscall/driver.rs` manages driver leases/handles, ownership, DMA resources, interrupt subscription, and endpoint notification. Exit cleanup explicitly drops process-owned leases.
- **Platform support**: `drivers/platform/` contains GPIO and PCIe ECAM-related code. Root compatibility exports preserve `drivers::pcie`; some cfg branches use the undeclared feature `pcie`.
- **VirtIO transport**: `drivers/virtio/` contains MMIO transport and split virtqueue code shared by device implementations. Transport existence is not a complete driver or DMA-safety proof.

## Storage and network

| Family | Sources | Boundary |
| --- | --- | --- |
| Block dispatch | `drivers/block/mod.rs`, `block/mod.rs` | Multi-sector helpers and block scheme/registration |
| AHCI | `drivers/block/ahci.rs` | Host/controller, command/FIS, read/write sources; no audited real-disk pass |
| NVMe | `drivers/block/nvme.rs` | Queue/controller and namespace I/O sources; hardware correctness unverified |
| VirtIO block | `block/virtio_blk.rs`, `drivers/virtio_blk.rs` | Raw implementation plus compatibility shim |
| NIC interface | `drivers/net/nic.rs` | Device abstraction and registration |
| Intel NIC | `drivers/net/e1000e.rs` | PCI/MMIO/ring implementation |
| VirtIO net | `drivers/net/virtio_net.rs` | Kernel PCI-oriented implementation |
| VirtIO-MMIO net | `drivers/net/virtio_net_mmio.rs` | Explicit compatibility stub |

The full networking graph is blocked by the duplicate socket module, so NIC source presence cannot establish application connectivity. See [networking](networking.md) and [storage](filesystems-and-storage.md) for dependent contracts.

## Graphics and display

`drivers/gpu/gpu.rs` defines a GPU HAL and backend registry; `gop.rs` captures firmware framebuffer information, and `framebuffer.rs` provides a common framebuffer abstraction. The x86 VGA text path is separately gated by architecture.

`drivers/gpu/drm.rs` is a skeletal hardware-facing DRM layer, while `display/drm/` defines the higher-level object model, buffers, GEM/PRIME handling, KMS, ioctl paths, and syscall bridge. These layers have different responsibilities and should not be documented as interchangeable implementations.

VirtIO GPU and feature-gated `amdgpu_gem.rs` code exist, but no renderer or hardware acceleration pass was established. `display/font.rs` and `display/console.rs` implement bitmap-font parsing/rasterization and framebuffer text rendering.

`display/wayland/` contains a compositor/server source tree gated by `wayland`. That feature is absent from root Cargo.toml, so a working compositor or supported `--features wayland` build must not be advertised.

## Input and terminal layers

- **Input drivers**: `drivers/input/` has evdev, HID, keyboard, mouse, USB, Bluetooth, and VirtIO-input sources. These are different backends and may contain partial paths.
- **Common events**: `input/mod.rs` owns event types and ring-buffer behavior, with `input_events` gating selected consumers. Buffer overflow behavior is part of that API.
- **Compatibility keyboard**: `drivers/keyboard.rs` remains a separate top-level source. Review actual callers before consolidating its behavior with `drivers/input/keyboard.rs`.
- **Terminals**: `tty/` contains line discipline, PTY, termios, and TTY syscall behavior. `serial_ldisc.rs` is not declared by `tty/mod.rs`, even though debug command code refers to it.
- **Console output**: `console/` provides early and routed serial output independently of the framebuffer/TTY stack. Minimal boot diagnostics cannot prove those higher layers.

## Media and camera prototypes

`drivers/media/media.rs` defines a media-device abstraction and backend registry. `drivers/media/isp4.rs` is gated by `amd-isp4`, but the root manifest does not declare that feature.

The source mentions an initialization hook, not a demonstrated firmware-loader, DMA, sensor, or capture pipeline. No support claim about a real camera, AMD platform, or upstream operating-system driver follows from those comments.

## Acceptance expectations

For each device, record exact identity, transport, firmware assumptions, interrupt mode, DMA/addressing limits, initialization result, and a bounded I/O workload. Test teardown, malformed responses, unavailable firmware, reset, timeout, and resource exhaustion separately.

Neither a PCI ID match nor a successful module compile proves hardware support. The [source index](module-map.md) includes every driver file so gaps can be tracked without overstating integration.
