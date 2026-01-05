use std::{ffi::CString, ptr::NonNull};

use macho::bindings::{self};

use crate::{
    ChainedFixup, FixupKind, Segment, Symbol, ThreadLocalKind, debug_println, mapping::MappedImage,
};

unsafe extern "C" {
    pub fn tlv_initialize_descriptors_export(mh: *const bindings::mach_header_64);
}

#[derive(Debug, Clone)]
pub struct Image {
    vm_addr: NonNull<u8>,
    start_addr: usize,
}

impl TryFrom<MappedImage> for Image {
    type Error = ();
    fn try_from(value: MappedImage) -> Result<Self, Self::Error> {
        let MappedImage {
            vm_addr,
            entry_point,
            page_zero_size,
            fixups,
            dependents,
            segments,
            symbols,
            init_functions,
            thread_locals,
        } = value;

        apply_fixups(
            vm_addr.as_ptr(),
            page_zero_size,
            &fixups,
            &dependents,
            &symbols,
        );

        debug_println!(
            "[   info]:{:>15}: applied {} fixups(s)",
            "relocations",
            fixups.len(),
        );

        segments_protect(vm_addr, &segments);
        handle_thread_locals(thread_locals, page_zero_size, vm_addr);
        init_functions_call(init_functions, page_zero_size, vm_addr);

        Ok(Self {
            vm_addr,
            start_addr: entry_point as usize + page_zero_size,
        })
    }
}

impl Image {
    pub fn offset_to_vm_addr(&self, offset: usize) -> NonNull<u8> {
        unsafe { self.vm_addr.add(offset) }
    }

    pub fn entry_point_addr(&self) -> NonNull<u8> {
        self.offset_to_vm_addr(self.start_addr)
    }
}

fn apply_fixups(
    dst_ptr: *mut u8,
    page_zero_size: usize,
    fixups: &[ChainedFixup],
    dependents: &[NonNull<u8>],
    symbols: &[Symbol],
) {
    for ChainedFixup {
        offset,
        kind: fixup_type,
    } in fixups
    {
        match &fixup_type {
            FixupKind::Rebase { target } => {
                unsafe {
                    let dst_addr = dst_ptr.add(*offset).add(page_zero_size) as *mut u64;
                    let target_addr = dst_ptr.add(*target as usize).add(page_zero_size).addr();
                    *dst_addr = target_addr as u64
                };
            }
            FixupKind::Bind {
                symbol_name,
                ordinal,
                is_auth: _is_auth,
                ..
            } => {
                match *ordinal {
                    bindings::BIND_SPECIAL_DYLIB_SELF => {
                        unimplemented!("BIND_SPECIAL_DYLIB_SELF");
                    }
                    bindings::BIND_SPECIAL_DYLIB_MAIN_EXECUTABLE => {
                        unimplemented!("BIND_SPECIAL_DYLIB_MAIN_EXECUTABLE");
                    }
                    bindings::BIND_SPECIAL_DYLIB_FLAT_LOOKUP => {
                        unimplemented!("BIND_SPECIAL_DYLIB_FLAT_LOOKUP");
                    }
                    bindings::BIND_SPECIAL_DYLIB_WEAK_LOOKUP => unsafe {
                        let dst_addr = dst_ptr.add(*offset).add(page_zero_size) as *mut u64;
                        let sym = symbols
                            .iter()
                            .find(|sym| &sym.name == symbol_name)
                            .expect("the symbol name should be stored");

                        *dst_addr = (dst_ptr.add(sym.impl_offset).addr()) as u64;
                    },
                    _ => {
                        let cstr = symbol_name
                            .strip_prefix("_")
                            .expect("symbol names are supposed to be prefixed with `_`");

                        let cstring = CString::new(cstr)
                            .expect("should be able to create a CString from a symbol name");

                        let dylib = dependents[*ordinal as usize - 1];

                        unsafe {
                            let dst_addr = dst_ptr.add(*offset).add(page_zero_size) as *mut u64;
                            *dst_addr =
                                libc::dlsym(dylib.as_ptr() as *mut libc::c_void, cstring.as_ptr())
                                    as u64;
                        };
                    }
                };
            }
        }
    }
}

fn segments_protect(dst_ptr: NonNull<u8>, segments: &[Segment]) {
    for Segment {
        vm_addr,
        vm_size,
        prot,
        ..
    } in segments
    {
        unsafe {
            [false, true].into_iter().for_each(|max| {
                kernutils::vm_protect(
                    dst_ptr.add(*vm_addr as usize).as_ptr().addr() as u64,
                    *vm_size,
                    max as i32,
                    *prot as i32,
                )
            });
        }
    }
}

fn init_functions_call(init_function_offsets: Vec<usize>, page_zero_size: usize, vm: NonNull<u8>) {
    for init_func_index in init_function_offsets {
        unsafe {
            jump::entry_and_ret(
                vm.add(init_func_index).add(page_zero_size),
                0,
                [core::ptr::null()].as_ptr(),
                *libc::_NSGetEnviron() as *const *const u8,
            )
        }
    }
}

fn handle_thread_locals(
    thread_locals: Vec<(usize, ThreadLocalKind)>,
    page_zero_size: usize,
    vm: NonNull<u8>,
) {
    if !thread_locals.is_empty() {
        unsafe {
            tlv_initialize_descriptors_export(
                vm.as_ptr().add(page_zero_size) as *const macho::bindings::mach_header_64
            );
        }
    }
}
