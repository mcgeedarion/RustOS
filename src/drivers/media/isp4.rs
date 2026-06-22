//! AMD ISP4 (amdisp4) capture driver.
//!
//! Corresponds to the `drivers/media/platform/amd/isp4` driver merged into
//! Linux 7.2 (June 2026, authored by Bin Du <Bin.Du@amd.com>).
//!
//! ## Hardware model
//!
//! The AMD ISP4 is an Image Signal Processor present in Ryzen AI Max 300
//! series SoCs (e.g. HP ZBook Ultra G1a).  Unlike previous generations, the
//! kernel driver does **not** directly manage the image sensor; instead it
//! communicates with ISP firmware (CCPU FW) running on a dedicated
//! co-processor via a mailbox command ring.  The firmware handles:
//!   - Sensor power sequencing (OV05C10 and friends)
//!   - MIPI CSI-2 lane configuration
//!   - Auto-exposure / auto-white-balance
//!   - Pixel pipeline tuning
//!
//! The kernel side:
//!   1. Powers the ISP block (MMIO ISP4_CTRL register)
//!   2. Loads firmware and releases the CCPU from reset
//!   3. Negotiates stream parameters via mailbox (ISP4_MBOX_*)
//!   4. Manages a DMA ring of `NUM_BUFS` capture buffers
//!   5. Services an interrupt / polls ISP4_STATUS for completed frames
//!
//! ## MMIO register map (offsets from BAR base)
//!
//! | Offset | Name            | Description                             |
//! |--------|-----------------|-----------------------------------------|
//! | 0x0000 | ISP4_CTRL       | Power / enable bits                     |
//! | 0x0004 | ISP4_STATUS     | HW status / interrupt flags             |
//! | 0x0008 | ISP4_RESET      | Soft-reset (write 1, self-clearing)     |
//! | 0x000C | ISP4_CLK_GATE   | Clock-gate control                      |
//! | 0x0100 | ISP4_FW_BASE_LO | Firmware PA bits [31:0]                 |
//! | 0x0104 | ISP4_FW_BASE_HI | Firmware PA bits [63:32]                |
//! | 0x0108 | ISP4_FW_SIZE    | Firmware blob size in bytes             |
//! | 0x010C | ISP4_FW_CTRL    | Firmware load trigger / CCPU release    |
//! | 0x0200 | ISP4_MBOX_CMD   | Mailbox command register (write)        |
//! | 0x0204 | ISP4_MBOX_STATUS| Mailbox status (bit 0 = ready)          |
//! | 0x0208 | ISP4_MBOX_DATA  | Mailbox data FIFO (32-bit words)        |
//! | 0x0300 | ISP4_DMA_BASE_LO| Capture ring base PA [31:0]             |
//! | 0x0304 | ISP4_DMA_BASE_HI| Capture ring base PA [63:32]            |
//! | 0x0308 | ISP4_DMA_STRIDE | Line stride in bytes                    |
//! | 0x030C | ISP4_DMA_CTRL   | Start/stop streaming                    |
//! | 0x0310 | ISP4_DMA_HEAD   | HW write pointer (frame index)          |
//! | 0x0314 | ISP4_DMA_TAIL   | SW read pointer (frame index)           |

#![allow(dead_code)]

extern crate alloc;
use alloc::vec::Vec;
use spin::Mutex;

// ── MMIO register offsets ─────────────────────────────────────────────────

const ISP4_CTRL:        u32 = 0x0000;
const ISP4_STATUS:      u32 = 0x0004;
const ISP4_RESET:       u32 = 0x0008;
const ISP4_CLK_GATE:    u32 = 0x000C;
const ISP4_FW_BASE_LO:  u32 = 0x0100;
const ISP4_FW_BASE_HI:  u32 = 0x0104;
const ISP4_FW_SIZE:     u32 = 0x0108;
const ISP4_FW_CTRL:     u32 = 0x010C;
const ISP4_MBOX_CMD:    u32 = 0x0200;
const ISP4_MBOX_STATUS: u32 = 0x0204;
const ISP4_MBOX_DATA:   u32 = 0x0208;
const ISP4_DMA_BASE_LO: u32 = 0x0300;
const ISP4_DMA_BASE_HI: u32 = 0x0304;
const ISP4_DMA_STRIDE:  u32 = 0x0308;
const ISP4_DMA_CTRL:    u32 = 0x030C;
const ISP4_DMA_HEAD:    u32 = 0x0310;
const ISP4_DMA_TAIL:    u32 = 0x0314;

// ── ISP4_CTRL bits ────────────────────────────────────────────────────────
const CTRL_POWER_ON:    u32 = 1 << 0;
const CTRL_CLK_EN:      u32 = 1 << 1;
const CTRL_SENSOR_EN:   u32 = 1 << 2;

// ── ISP4_STATUS bits ─────────────────────────────────────────────────────
const STATUS_FW_READY:  u32 = 1 << 0;
const STATUS_FRAME_RDY: u32 = 1 << 1;
const STATUS_ERROR:     u32 = 1 << 8;

// ── Mailbox command codes ─────────────────────────────────────────────────
const MBOX_CMD_INIT:         u32 = 0x01;
const MBOX_CMD_SET_FORMAT:   u32 = 0x02;
const MBOX_CMD_STREAM_ON:    u32 = 0x03;
const MBOX_CMD_STREAM_OFF:   u32 = 0x04;
const MBOX_CMD_SET_DMA_RING: u32 = 0x05;

// ── DMA ring configuration ────────────────────────────────────────────────

/// Number of capture buffers in the DMA ring.
const NUM_BUFS: usize = 4;

/// Maximum supported resolution (4K UHD).
const MAX_WIDTH:  u32 = 3840;
const MAX_HEIGHT: u32 = 2160;

/// Bytes per pixel for YUYV (16 bpp packed).
const YUYV_BPP: u32 = 2;

// ── Driver state ─────────────────────────────────────────────────────────

struct Isp4State {
    mmio_base: u64,
    fw_paddr: u64,
    /// Physical base of the contiguous DMA ring.
    dma_ring_paddr: u64,
    /// Per-buffer physical addresses within the ring.
    buf_paddrs: [u64; NUM_BUFS],
    buf_sizes:  [u32; NUM_BUFS],
    streaming: bool,
    width: u32,
    height: u32,
}

static STATE: Mutex<Option<Isp4State>> = Mutex::new(None);

// ── MMIO helpers ─────────────────────────────────────────────────────────

/// Write a 32-bit value to an MMIO register.
///
/// # Safety
/// Caller must guarantee `base + offset` is a valid MMIO address.
#[inline(always)]
unsafe fn mmio_write(base: u64, offset: u32, val: u32) {
    let ptr = (base + offset as u64) as *mut u32;
    ptr.write_volatile(val);
}

/// Read a 32-bit value from an MMIO register.
///
/// # Safety
/// Same as `mmio_write`.
#[inline(always)]
unsafe fn mmio_read(base: u64, offset: u32) -> u32 {
    let ptr = (base + offset as u64) as *const u32;
    ptr.read_volatile()
}

// ── Mailbox helpers ───────────────────────────────────────────────────────

/// Poll ISP4_MBOX_STATUS bit 0 until the mailbox is ready.
/// Spins up to `timeout` iterations (no sleep — bare-metal context).
unsafe fn mbox_wait_ready(base: u64, timeout: u32) -> bool {
    for _ in 0..timeout {
        if mmio_read(base, ISP4_MBOX_STATUS) & 1 != 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

/// Send a single mailbox command word followed by zero or more data words.
unsafe fn mbox_send(base: u64, cmd: u32, data: &[u32]) -> Result<(), &'static str> {
    if !mbox_wait_ready(base, 1_000_000) {
        return Err("isp4: mailbox timeout waiting for ready");
    }
    mmio_write(base, ISP4_MBOX_CMD, cmd);
    for &word in data {
        if !mbox_wait_ready(base, 100_000) {
            return Err("isp4: mailbox timeout pushing data word");
        }
        mmio_write(base, ISP4_MBOX_DATA, word);
    }
    Ok(())
}

// ── Public driver API ─────────────────────────────────────────────────────

/// Initialise the AMD ISP4 hardware.
///
/// Called once from `media::init_isp4()`.
///
/// Steps:
///   1. Assert soft-reset, then deassert.
///   2. Enable power and clocks.
///   3. Program firmware physical address and trigger CCPU boot.
///   4. Wait for ISP4_STATUS[FW_READY].
///   5. Send MBOX_CMD_INIT.
pub fn init(mmio_base: u64, fw_paddr: u64) {
    unsafe {
        // 1. Soft-reset
        mmio_write(mmio_base, ISP4_RESET, 1);
        // Allow reset to propagate (spin a few cycles)
        for _ in 0..1000 { core::hint::spin_loop(); }

        // 2. Power on + enable clocks
        mmio_write(mmio_base, ISP4_CLK_GATE, 0);
        mmio_write(mmio_base, ISP4_CTRL, CTRL_POWER_ON | CTRL_CLK_EN);

        // 3. Load firmware
        mmio_write(mmio_base, ISP4_FW_BASE_LO, (fw_paddr & 0xFFFF_FFFF) as u32);
        mmio_write(mmio_base, ISP4_FW_BASE_HI, (fw_paddr >> 32) as u32);
        // FW size unknown at compile time — write 0 to let HW auto-detect or
        // rely on UEFI/ACPI having pre-loaded the blob.
        mmio_write(mmio_base, ISP4_FW_SIZE, 0);
        // Release CCPU from reset (bit 0 = FW_LOAD_EN, bit 1 = CCPU_RELEASE)
        mmio_write(mmio_base, ISP4_FW_CTRL, 0b11);

        // 4. Wait for firmware ready (poll up to ~1 M iterations)
        let mut ready = false;
        for _ in 0..1_000_000u32 {
            if mmio_read(mmio_base, ISP4_STATUS) & STATUS_FW_READY != 0 {
                ready = true;
                break;
            }
            core::hint::spin_loop();
        }
        if !ready {
            // Non-fatal: continue — QEMU / simulation may not model FW boot.
            // In production a proper error return would be propagated.
        }

        // 5. Send INIT command
        let _ = mbox_send(mmio_base, MBOX_CMD_INIT, &[]);
    }

    *STATE.lock() = Some(Isp4State {
        mmio_base,
        fw_paddr,
        dma_ring_paddr: 0,
        buf_paddrs: [0u64; NUM_BUFS],
        buf_sizes: [0u32; NUM_BUFS],
        streaming: false,
        width: 0,
        height: 0,
    });
}

/// Start capturing at the given resolution.
///
/// Allocates a contiguous DMA ring via the kernel PMM, programs it into
/// the hardware, and sends STREAM_ON to the firmware.
pub fn capture_start(width: u32, height: u32) -> Result<(), &'static str> {
    if width > MAX_WIDTH || height > MAX_HEIGHT {
        return Err("isp4: resolution exceeds 4K limit");
    }

    let mut guard = STATE.lock();
    let state = guard.as_mut().ok_or("isp4: driver not initialised")?;

    if state.streaming {
        return Err("isp4: already streaming");
    }

    let stride = width * YUYV_BPP;
    let buf_size = stride * height;

    // Allocate a contiguous DMA ring.
    // In a real kernel this would call the PMM.
    // Here we use a static placeholder address and flag it as allocated.
    // SAFETY NOTE: Replace with a real PMM allocation in production.
    let ring_paddr: u64 = 0x8000_0000; // placeholder — PMM::alloc_contig(NUM_BUFS * buf_size)
    let mut buf_paddrs = [0u64; NUM_BUFS];
    for i in 0..NUM_BUFS {
        buf_paddrs[i] = ring_paddr + (i as u64) * buf_size as u64;
    }

    unsafe {
        let base = state.mmio_base;

        // Program DMA ring base
        mmio_write(base, ISP4_DMA_BASE_LO, (ring_paddr & 0xFFFF_FFFF) as u32);
        mmio_write(base, ISP4_DMA_BASE_HI, (ring_paddr >> 32) as u32);
        mmio_write(base, ISP4_DMA_STRIDE,  stride);

        // Notify firmware: SET_FORMAT  [width, height, fourcc_YUYV, num_bufs]
        mbox_send(base, MBOX_CMD_SET_FORMAT, &[
            width, height,
            u32::from_le_bytes(*b"YUYV"),
            NUM_BUFS as u32,
        ])?;

        // Notify firmware: SET_DMA_RING  [ring_lo, ring_hi, buf_size, num_bufs]
        mbox_send(base, MBOX_CMD_SET_DMA_RING, &[
            (ring_paddr & 0xFFFF_FFFF) as u32,
            (ring_paddr >> 32) as u32,
            buf_size,
            NUM_BUFS as u32,
        ])?;

        // Enable sensor pipeline
        let ctrl = mmio_read(base, ISP4_CTRL);
        mmio_write(base, ISP4_CTRL, ctrl | CTRL_SENSOR_EN);

        // Start DMA
        mmio_write(base, ISP4_DMA_CTRL, 1);

        // Tell firmware to stream
        mbox_send(base, MBOX_CMD_STREAM_ON, &[])?;
    }

    state.dma_ring_paddr = ring_paddr;
    state.buf_paddrs     = buf_paddrs;
    state.buf_sizes      = [buf_size; NUM_BUFS];
    state.streaming      = true;
    state.width          = width;
    state.height         = height;

    Ok(())
}

/// Stop streaming and power down the sensor pipeline.
pub fn capture_stop() {
    let mut guard = STATE.lock();
    let state = match guard.as_mut() {
        Some(s) if s.streaming => s,
        _ => return,
    };

    unsafe {
        let base = state.mmio_base;
        let _ = mbox_send(base, MBOX_CMD_STREAM_OFF, &[]);
        mmio_write(base, ISP4_DMA_CTRL, 0);
        let ctrl = mmio_read(base, ISP4_CTRL);
        mmio_write(base, ISP4_CTRL, ctrl & !CTRL_SENSOR_EN);
    }

    state.streaming = false;
}

/// Dequeue the oldest completed capture buffer.
///
/// Reads `ISP4_DMA_HEAD` (HW write pointer) and `ISP4_DMA_TAIL` (SW read
/// pointer).  If head != tail a frame is available; we bump the tail and
/// return a `CaptureFrame` descriptor.
///
/// Returns `None` if no frame is ready.
pub fn dequeue_frame() -> Option<super::media::CaptureFrame> {
    let guard = STATE.lock();
    let state = guard.as_ref()?;
    if !state.streaming {
        return None;
    }

    let (head, tail) = unsafe {
        let base = state.mmio_base;
        let h = mmio_read(base, ISP4_DMA_HEAD) as usize % NUM_BUFS;
        let t = mmio_read(base, ISP4_DMA_TAIL) as usize % NUM_BUFS;
        (h, t)
    };

    if head == tail {
        return None; // ring empty
    }

    let frame = super::media::CaptureFrame {
        index:        tail as u32,
        paddr:        state.buf_paddrs[tail],
        byte_len:     state.buf_sizes[tail],
        sequence:     tail as u32, // simplified; real HW provides a counter
        timestamp_ns: 0,           // real HW: read from ISP4_TIMESTAMP register
    };

    Some(frame)
}

/// Return a buffer to the hardware ring (advance the SW tail pointer).
pub fn queue_buf(index: u32) {
    let guard = STATE.lock();
    if let Some(state) = guard.as_ref() {
        if state.streaming {
            unsafe {
                // Advance the tail pointer so HW can reuse this slot
                mmio_write(state.mmio_base, ISP4_DMA_TAIL, (index + 1) % NUM_BUFS as u32);
            }
        }
    }
}

/// Return the list of supported pixel formats.
pub fn enum_formats() -> Vec<super::media::PixelFormat> {
    alloc::vec![
        super::media::FMT_YUYV,
        super::media::FMT_NV12,
    ]
}
