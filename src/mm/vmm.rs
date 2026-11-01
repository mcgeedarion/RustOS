//! Reference-counted physical backing for userspace shared mappings.
extern crate alloc;
use alloc::collections::BTreeMap;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use crate::arch::{api::Paging, Arch};
use crate::mm::mmap::{self, Vma, VmaKind, PAGE};
use crate::proc::scheduler;

/// Owns each frame exactly once. Every VMA and IPC object retains an Arc.
#[derive(Debug)]
pub struct SharedFrames {
    frames: Vec<usize>,
}

impl SharedFrames {
    /// Transfer exclusive ownership of PMM frames into a shared backing.
    /// Callers must not independently free the transferred frames.
    pub fn from_owned(frames: Vec<usize>) -> Self {
        assert!(frames.iter().all(|pa| *pa != 0 && pa & (PAGE - 1) == 0));
        assert!(frames.iter().enumerate().all(|(i, pa)| !frames[..i].contains(pa)));
        Self { frames }
    }

    pub fn frames(&self) -> &[usize] { &self.frames }
}

impl Drop for SharedFrames {
    fn drop(&mut self) {
        for &pa in &self.frames {
            crate::mm::pmm::free_page(pa);
        }
    }
}

#[derive(Clone)]
struct Attachment {
    len: usize,
    owner: Weak<SharedFrames>,
}

static ATTACHMENTS: spin::Mutex<BTreeMap<(usize, usize), Attachment>> =
    spin::Mutex::new(BTreeMap::new());

fn aligned_len(size: usize) -> Result<usize, isize> {
    if size == 0 { return Err(-22); }
    size.checked_add(PAGE - 1).map(|n| n & !(PAGE - 1)).ok_or(-22)
}

pub fn map_shared(
    hint: usize,
    size: usize,
    prot: u32,
    owner: Arc<SharedFrames>,
) -> Result<usize, isize> {
    let len = aligned_len(size)?;
    if len / PAGE > owner.frames.len() || prot & !7 != 0 || hint & (PAGE - 1) != 0 {
        return Err(-22);
    }
    let pid = scheduler::current_pid();
    let va = mmap::with_mm_write(pid, |pcb| {
        if pcb.user_pagetable == 0 { return Err(-14); }
        let used = pcb.vmas.iter().fold(0usize, |n, v| n.saturating_add(v.end - v.start));
        if pcb.rlimits.exceeds_as(used, len) { return Err(-12); }
        let mut va = if hint == 0 { pcb.next_va.max(PAGE) } else { hint };
        loop {
            let end = va.checked_add(len).ok_or(-12isize)?;
            if end > crate::uaccess::USER_SPACE_END || va < PAGE { return Err(-12); }
            match pcb.vmas.iter().filter(|v| v.start < end && v.end > va).map(|v| v.end).max() {
                Some(next) if hint == 0 => va = aligned_len(next)?,
                Some(_) => return Err(-17),
                None => break,
            }
        }
        let flags = mmap::protection::prot_to_flags(prot);
        for (index, &pa) in owner.frames.iter().take(len / PAGE).enumerate() {
            if !<Arch as Paging>::map_page(pcb.user_pagetable, va + index * PAGE, pa, flags) {
                for done in 0..index {
                    <Arch as Paging>::unmap_page(pcb.user_pagetable, va + done * PAGE);
                }
                crate::smp::ipi::tlb_shootdown(va as u64, (va + len) as u64, 0);
                return Err(-12);
            }
        }
        for index in 0..len / PAGE {
            pcb.vmas.push(Vma {
                start: va + index * PAGE,
                end: va + (index + 1) * PAGE,
                prot,
                flags: mmap::MAP_SHARED,
                kind: VmaKind::Shared(Arc::clone(&owner), index),
                file_offset: 0,
                locked: false,
            });
        }
        pcb.vmas.sort_by_key(|v| v.start);
        pcb.next_va = pcb.next_va.max(va.saturating_add(len).saturating_add(PAGE));
        Ok(va)
    }).ok_or(-3isize)??;
    ATTACHMENTS.lock().insert((pid, va), Attachment { len, owner: Arc::downgrade(&owner) });
    Ok(va)
}

/// Attach borrowed frames by acquiring a PMM reference for each one.
pub fn map_range(hint: usize, size: usize, prot: u32, frames: &[usize]) -> Result<usize, isize> {
    let len = aligned_len(size)?;
    if len / PAGE > frames.len() { return Err(-22); }
    for &pa in frames { crate::mm::pmm::get_page(pa); }
    map_shared(hint, size, prot, Arc::new(SharedFrames::from_owned(frames.to_vec())))
}

pub fn unmap_range(addr: usize) -> Result<(), isize> {
    let pid = scheduler::current_pid();
    let attachment = ATTACHMENTS.lock().remove(&(pid, addr)).ok_or(-22isize)?;
    let Some(owner) = attachment.owner.upgrade() else { return Ok(()); };
    mmap::with_mm_write(pid, |pcb| {
        // Remove only VMAs of the same backing. A user may already have unmapped
        // this range and installed an unrelated mapping at the original address.
        let end = addr + attachment.len;
        for vma in &pcb.vmas {
            if vma.start >= addr && vma.end <= end {
                if let VmaKind::Shared(backing, _) = &vma.kind {
                    if Arc::ptr_eq(backing, &owner) {
                        <Arch as Paging>::unmap_page(pcb.user_pagetable, vma.start);
                    }
                }
            }
        }
        crate::smp::ipi::tlb_shootdown(addr as u64, end as u64, 0);
        pcb.vmas.retain(|vma| {
            !(vma.start >= addr && vma.end <= end
                && matches!(&vma.kind, VmaKind::Shared(backing, _) if Arc::ptr_eq(backing, &owner)))
        });
    }).ok_or(-3isize)?;
    Ok(())
}

pub fn fork_attach(parent: usize, child: usize) {
    let mut table = ATTACHMENTS.lock();
    let inherited: Vec<_> = table.iter().filter(|((pid, _), _)| *pid == parent)
        .map(|((_, addr), entry)| (*addr, entry.clone())).collect();
    for (addr, entry) in inherited { table.insert((child, addr), entry); }
}

pub fn forget_process(pid: usize) {
    ATTACHMENTS.lock().retain(|(owner, _), _| *owner != pid);
}
