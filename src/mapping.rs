use std::{ffi::CString, ptr::NonNull, str::FromStr};

use debug_println::debug_println;
use libc::{RTLD_LOCAL, RTLD_NOW};

use crate::{ChainedFixup, Segment, Symbol, ThreadLocalKind, parsing::ParsedImage};

#[derive(Debug, Clone)]
pub struct MappedImage {
    pub dependents: Vec<NonNull<u8>>,
    pub symbols: Vec<Symbol>,
    pub entry_point: u64,
    pub page_zero_size: usize,
    pub fixups: Vec<ChainedFixup>,
    pub init_functions: Vec<usize>,
    pub thread_locals: Vec<(usize, ThreadLocalKind)>,
    pub segments: Vec<Segment>,
    pub vm_addr: NonNull<u8>,
}

impl TryFrom<ParsedImage<'_>> for MappedImage {
    type Error = ();

    fn try_from(value: ParsedImage) -> Result<Self, Self::Error> {
        let (vm_addr, _) = address_space_init(value.as_bytes(), &value.segments);
        let fixups = value.fixups;

        Ok(Self {
            dependents: dependents_init(&value.libraries),
            entry_point: value.entry_point as u64,
            symbols: value.symbols,
            page_zero_size: value.segments[0].vm_size,
            init_functions: value.init_functions,
            thread_locals: value.thread_locals,
            segments: value.segments,
            fixups,
            vm_addr,
        })
    }
}

fn address_space_size(segments: &[Segment]) -> usize {
    let seg_bounds = segments.iter().map(
        |Segment {
             vm_addr, vm_size, ..
         }| { (vm_addr, *vm_addr + *vm_size as u64) },
    );
    let (min_addr, max_addr) = seg_bounds.fold((u64::MAX, 0), |(min, max), (start, end)| {
        (min.min(*start), max.max(end))
    });
    (max_addr - min_addr) as usize
}

pub fn address_space_init(image: &[u8], segments: &[Segment]) -> (NonNull<u8>, usize) {
    let vm_size = address_space_size(segments);
    let memory = kernutils::vm_alloc_task_self(vm_size);

    debug_println!(
        "[   info]:{:>15}: allocated {vm_size} byte(s) of vm",
        "vm-mapping"
    );
    for Segment {
        vm_addr,
        vm_size,
        file_offset,
        ..
    } in segments
    {
        unsafe {
            kernutils::vm_copy_into_task_self(
                image.as_ptr().add(*file_offset).addr() as u64,
                memory.add(*vm_addr as usize).as_ptr().addr() as u64,
                *vm_size,
            );

            debug_println!(
                "[   info]:{:>15}: mapping offset 0x{:x} at vm 0x{:x} ({vm_size} byte(s))",
                "vm-mapping",
                image.as_ptr().add(*file_offset).addr() as u64,
                memory.add(*vm_addr as usize).as_ptr().addr() as u64,
            );
        }
    }

    (memory, vm_size)
}

pub fn dependents_init(libraries: &[(String, usize)]) -> Vec<NonNull<u8>> {
    let mut dependents = Vec::new();
    for (dylib_name, _) in libraries {
        let cstring =
            CString::from_str(dylib_name).expect("a dylib name cannot contain a null byte");

        let dylib_ptr = unsafe { libc::dlopen(cstring.as_ptr(), RTLD_NOW | RTLD_LOCAL) as *mut u8 };
        if dylib_ptr.is_null() {
            panic!("failed to load dylib @ {dylib_ptr:?}");
        }

        debug_println!(
            "[   info]:{:>15}: dlopen {cstring:?} @ 0x{:x}",
            "vm-mapping",
            dylib_ptr.addr()
        );

        dependents.push(NonNull::new(dylib_ptr).unwrap());
    }
    dependents
}
