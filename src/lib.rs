use std::ffi::CStr;

use debug_println::debug_println;
use macho::{bindings, container::Container};

use crate::{mapping::MappedImage, parsing::ParsedImage, relocations::Image};

pub mod mapping;
pub mod parsing;
pub mod relocations;

/// entry point for the loader.
#[unsafe(no_mangle)]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn execvm(
    ac: u32,
    argv: *const *const u8,
    envp: *const *const u8,
    data: *const u8,
    len: usize,
) {
    unsafe {
        //let program_path = CStr::from_ptr(*argv as *mut i8);
        let data_slice: &[u8] = core::slice::from_raw_parts(data, len);
        let loader = ImageLoader::with_container(Container::with_bytes(data_slice));

        loader
            .map_segments()
            .apply_relocations()
            .transfer_control(ac, argv, envp);
    }
}

#[derive(Debug, Clone)]
pub enum ImageLoader<'image> {
    Parsed(ParsedImage<'image>),
    AddressSpaceMapped(MappedImage),
    Relocated(Image),
}

impl<'image> ImageLoader<'image> {
    pub fn with_container(container: Container<'image>) -> Self {
        Self::Parsed(ParsedImage::with_container(container))
    }

    pub fn map_segments(mut self) -> Self {
        if let ImageLoader::Parsed(parsed) = self {
            self = ImageLoader::AddressSpaceMapped(MappedImage::try_from(parsed).unwrap());
            return self;
        }
        panic!("only parsed images can be mapped");
    }

    pub fn apply_relocations(mut self) -> Self {
        if let ImageLoader::AddressSpaceMapped(image) = self {
            self = ImageLoader::Relocated(Image::try_from(image).unwrap());
            return self;
        }

        panic!("only address space mapped images can be relocated");
    }

    #[allow(clippy::missing_safety_doc)]
    pub unsafe fn transfer_control(
        self,
        argc: u32,
        argv: *const *const u8,
        envp: *const *const u8,
    ) -> ! {
        if let ImageLoader::Relocated(image) = self {
            unsafe {
                debug_println!(
                    "[   info]:{:>15}: transfering control to entrypoint @ 0x{:x}...\n",
                    "loader",
                    image.entry_point_addr().addr()
                );
                jump::entry(image.entry_point_addr(), argc as usize, argv, envp);
            }
        }
        panic!("only relocated images can be executed")
    }
}

#[derive(Debug, Clone)]
pub struct ChainedFixup {
    offset: usize,
    kind: FixupKind,
}

#[derive(Clone, Debug)]
enum FixupKind {
    Rebase {
        target: u64,
    },
    Bind {
        symbol_name: String,
        ordinal: u32,
        is_auth: bool,
        #[allow(unused)]
        addend: i64,
    },
}
impl ChainedFixup {
    pub fn new_rebase(offset: usize, target: u64) -> Self {
        ChainedFixup {
            offset,
            kind: FixupKind::Rebase { target },
        }
    }

    pub fn new_bind(
        offset: usize,
        symbol_name: String,
        ordinal: u32,
        addend: i64,
        is_auth: bool,
    ) -> Self {
        ChainedFixup {
            offset,
            kind: FixupKind::Bind {
                symbol_name,
                ordinal,
                is_auth,
                addend,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct Segment {
    file_offset: usize,
    vm_addr: u64,
    vm_size: usize,
    prot: u32,
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct Symbol {
    lib_ordinal: u8,
    name: String,
    impl_offset: usize,
    sect_num: u8,
    value: usize,
    is_weak: bool,
    is_thumb: bool,
    is_cold: bool,
}

#[derive(Debug, Clone)]
pub enum ThreadLocalKind {
    Variable,
    Regular,
    Zerofill,
    InitFunctionPointers,
}

impl Symbol {
    pub fn make_undefined(name: String, lib_ordinal: u8, is_weak: bool) -> Self {
        Self {
            lib_ordinal,
            name,
            impl_offset: 0,
            sect_num: 0,
            value: 0,
            is_weak,
            is_cold: false,
            is_thumb: false,
        }
    }

    pub fn make_regular_local(
        name: String,
        image_offset: usize,
        sect_num: u8,
        is_cold: bool,
        is_thumb: bool,
    ) -> Self {
        Self {
            lib_ordinal: bindings::SELF_LIBRARY_ORDINAL,
            name,
            value: 0,
            impl_offset: image_offset,
            sect_num,
            is_weak: false,
            is_cold,
            is_thumb,
        }
    }

    pub fn make_regular_export(
        name: String,
        image_offset: usize,
        sect_num: u8,
        is_cold: bool,
        is_thumb: bool,
    ) -> Self {
        Self {
            lib_ordinal: bindings::SELF_LIBRARY_ORDINAL,
            name,
            value: 0,
            impl_offset: image_offset,
            sect_num,
            is_weak: false,
            is_cold,
            is_thumb,
        }
    }

    pub fn make_weak_def_export(
        name: String,
        image_offset: usize,
        sect_num: u8,
        is_cold: bool,
        is_thumb: bool,
    ) -> Self {
        Self {
            lib_ordinal: bindings::SELF_LIBRARY_ORDINAL,
            name,
            value: 0,
            impl_offset: image_offset,
            sect_num,
            is_weak: true,
            is_cold,
            is_thumb,
        }
    }
}
