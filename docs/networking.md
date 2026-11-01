# Networking

The network tree contains protocol implementations and two competing socket-module layouts. At `08a14f6`, the full build reports Rust error E0761 because `net/mod.rs` declares `socket` while both `net/socket.rs` and `net/socket/mod.rs` exist.

## Packet and control paths

```text
NIC driver -> Ethernet -> IPv4 / IPv6
                         ICMP / ICMPv6
                         UDP / TCP
                           socket and scheme interfaces
                           DHCP / DNS helpers
```

- **Ethernet and ARP**: `eth.rs` handles frame dispatch and MAC addresses; `arp.rs` has a cache and pending-packet queue. Queued resolution does not imply complete retry/aging behavior.
- **IPv4 and ICMP**: `ip.rs` implements address/gateway/mask state, header/checksum handling, and send/receive routing. `icmp.rs` contains echo-reply and unreachable handling.
- **IPv6 and ICMPv6**: `ipv6.rs` covers address helpers, extension-header walking, receive-side fragment handling, and send paths. `icmpv6.rs` contains neighbor/router discovery and echo support; transmit fragmentation of oversized IPv6 payloads is not implemented.
- **UDP**: `udp.rs` manages port registrations, checksums, receive demultiplexing, and the UDP scheme. Socket-facing IPv4/IPv6 UDP helpers live in `socket/udp/mod.rs`.
- **TCP**: `tcp.rs` contains connection slots, handshake/state handling, send/receive, closure, retransmission tick code, and the TCP scheme. The separate `tcp/state_machine.rs` file is not declared by `tcp.rs`.
- **DHCP**: `dhcp.rs` builds requests, handles replies, and updates lease-derived configuration. A background lease-renewal task is not implemented.
- **DNS**: `dns.rs` is a stub resolver in the networking sense: it has actual A/AAAA query, reply, name decoding, caching, and wait logic. “Stub resolver” here does not mean an empty implementation.

## Socket subtree

The directory-based socket implementation separates addresses, Unix buffers, types, core creation/bind/listen/accept/connect, poll integration, syscall glue, options, TCP helpers, UDP handling, and Unix streams. Its intended public re-exports are visible in `socket/mod.rs`.

The competing `socket.rs` defines a different socket abstraction and imports the undeclared TCP state-machine path. Documentation cannot select a winner: the source must first resolve module ownership and reconcile consumers before either can be reported as the compiled socket API.

## Devices and schemes

`drivers/net/mod.rs` probes supported PCI identifiers on x86_64 and prefers the e1000e path before legacy/transitional VirtIO net. The NIC abstraction and driver implementations live in `nic.rs`, `e1000e.rs`, and `virtio_net.rs`.

`virtio_net_mmio.rs` is a compatibility stub, not a working alternative transport. The userspace VirtIO-net prototype under `userspace/drivers/` is also not a packaged, validated replacement for these kernel drivers.

`NetScheme` exposes network status/configuration, while TCP and UDP schemes provide protocol-oriented resource access. Their registration in `init/schemes.rs` is independent of successful NIC initialization or protocol interoperability.

## Validation boundary

No real packet exchange, DHCP lease renewal, TCP congestion behavior, socket interoperability, or network-namespace isolation pass was established in this audit. Full compilation is already blocked by socket-module ambiguity, so network support should be described as implementation work rather than a tested capability.

Future tests should use bounded request/reply workloads, malformed/truncated packets, queue pressure, timeouts, and close/reset behavior. Keep host parser/unit evidence distinct from QEMU network-device and multi-process socket evidence.
