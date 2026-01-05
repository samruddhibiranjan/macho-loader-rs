use std::{ffi::CStr, str::FromStr};

use libc::load_command;
use macho::{
    bindings::{self, get_library_ordinal},
    container::Container,
    name_eq,
};

use crate::{ChainedFixup, FixupKind, Segment, Symbol, ThreadLocalKind, debug_println};
/// A parsed arm64 image
///
/// this struct is created from a [`Container`] with
/// `with_container`.
#[derive(Clone, Debug)]
pub struct ParsedImage<'image> {
    /// the innner container
    ///
    /// this holds the image bytes corresponding
    /// to this parsed image
    container: Container<'image>,

    /// the segments within the inner binary
    ///
    /// this vector holds the parsed segments, in the order in
    /// which they appear in the binary.
    pub segments: Vec<Segment>,

    /// the symbols within the inner binary
    ///
    /// this vector holds the parsed symbols, in the order in
    /// which they appear in the symbol table.
    pub symbols: Vec<Symbol>,

    /// the entry point offset of the binary.
    ///
    /// this is the entrypoint of the program, it is an offset from the
    /// start of the image.
    pub entry_point: usize,

    /// the dynamic libraries used by the binary.
    ///
    /// this vector holds the name and the offset of the
    /// libraries used by the program.
    pub libraries: Vec<(String, usize)>,

    /// the thread local variables used by this binary.
    ///
    /// this vector holds the thread local variables offset and kind.
    pub thread_locals: Vec<(usize, ThreadLocalKind)>,

    /// The parsed chained fixups of the binary.
    pub fixups: Vec<ChainedFixup>,

    /// the constructor function pointers of the program.
    ///
    /// This vector holds the offsets of all the init functions
    /// contained in the inner binary.
    pub init_functions: Vec<usize>,
}

impl<'image> ParsedImage<'image> {
    pub fn as_bytes(&self) -> &'image [u8] {
        self.container.as_bytes()
    }
}

impl<'image> ParsedImage<'image> {
    pub fn with_container(container: Container<'image>) -> Self {
        if is_of_arch(&container, bindings::CPU_TYPE_ARM64) {
            debug_println!("[   info]:{:>15}: found CPU_TYPE_ARM64 arch", "mach-o");
            let mut parsed = Self {
                container,
                segments: Vec::new(),
                symbols: Vec::new(),
                init_functions: Vec::new(),
                thread_locals: Vec::new(),
                entry_point: 0,
                libraries: Vec::new(),
                fixups: Vec::new(),
            };
            parsed.handle_load_commands();
            parsed
        } else {
            match container.read_type(0) {
                bindings::mach_magic::FAT_MAGIC => {
                    Self::extract_arch(container, bindings::CPU_TYPE_ARM64)
                        .expect("binary must contain a valid AMR64 slice")
                }
                _ => panic!("Not a valid ARM64 Mach-O or fat binary"),
            }
        }
    }

    fn extract_arch(container: Container<'image>, arch: i32) -> Option<Self> {
        let header = container.read_type::<bindings::fat_header>(0);
        debug_println!(
            "[   info]:{:>15}: iterating over {} architectur(e) ...",
            "mach-o",
            header.nfat_arch
        );

        (0..header.nfat_arch)
            .filter_map(|i| create_arch_container(&container, i))
            .find(|container| is_of_arch(container, arch))
            .map(Self::with_container)
    }

    fn handle_load_commands(&mut self) {
        let bindings::mach_header_64 {
            filetype, ncmds, ..
        } = self.container.read_type(0);

        // we only support basic executables for now, everything
        // else should panic
        if !matches!(filetype, bindings::macho_filetype_variants::MH_EXECUTE) {
            unimplemented!("LC: filetype: {filetype:?}")
        }

        let mut offset = size_of::<bindings::mach_header_64>();

        debug_println!(
            "[   info]:{:>15}: iterating over {ncmds} load command(s) ...",
            "mach-o"
        );

        for _ in 0..ncmds {
            let bindings::load_command { cmd, cmdsize } = self.container.read_type(offset);
            match cmd {
                bindings::load_command_variants::LC_LOAD_DYLIB
                | bindings::load_command_variants::LC_LOAD_WEAK_DYLIB
                | bindings::load_command_variants::LC_LAZY_LOAD_DYLIB => {
                    self.handle_load_dylib(offset);
                }
                bindings::load_command_variants::LC_MAIN => {
                    self.handle_main(offset);
                }
                bindings::load_command_variants::LC_SEGMENT_64 => {
                    self.handle_segment_64(offset);
                }
                bindings::load_command_variants::LC_SYMTAB => {
                    self.handle_symtab(offset);
                }
                bindings::load_command_variants::LC_DYLD_CHAINED_FIXUPS => {
                    self.handle_chained_fixups(offset);
                }
                bindings::load_command_variants::LC_RPATH => {
                    self.handle_rpath(offset);
                }
                _ => {
                    debug_println!(
                        "[warning]:{:>15}: skipping unsupported {cmd:?} at offset 0x{offset:x} (sizes {cmdsize})",
                        "mach-o"
                    );
                }
            }
            offset += cmdsize as usize;
        }
    }

    fn handle_load_dylib(&mut self, offset: usize) {
        let bindings::dylib_command { dylib, .. } = self.container.read_type(offset);

        let name = self.container.read_str(offset + dylib.name.offset as usize);

        debug_println!(
            "[   info]:{:>15}: dylib_command @ 0x{:x}: loading '{name}'",
            "mach-o",
            offset
        );

        self.libraries.push((
            String::from_str(name).unwrap(),
            offset + dylib.name.offset as usize,
        ));
    }

    fn handle_main(&mut self, offset: usize) {
        let bindings::entry_point_command { entryoff, .. } = self.container.read_type(offset);
        debug_println!(
            "[   info]:{:>15}: entry_point_command @ 0x{:x}: entryoff: 0x{entryoff}",
            "mach-o",
            offset
        );
        self.entry_point = entryoff as usize;
    }

    fn handle_segment_64(&mut self, offset: usize) {
        let bindings::segment_command_64 {
            initprot,
            vmaddr: vm_addr,
            vmsize: vm_size,
            fileoff: file_off,
            segname,
            nsects,
            ..
        } = self.container.read_type(offset);

        let seg = CStr::from_bytes_until_nul(&segname).unwrap_or_default();
        debug_println!(
            "[   info]:{:>15}: segment_command_64 ({seg:?}) @ 0x{file_off:x}: vmaddr: 0x{vm_addr:x}, vmsize: {vm_size}",
            "mach-o"
        );

        if name_eq(&segname, bindings::SEG_TEXT) {
            self.handle_text_segment(offset, nsects as usize);
        } else if name_eq(&segname, bindings::SEG_DATA) {
            self.handle_data_segment(offset, nsects as usize);
        }

        self.segments.push(Segment {
            prot: initprot,
            vm_addr,
            file_offset: file_off as usize,
            vm_size: vm_size as usize,
        });
    }

    fn handle_text_segment(&mut self, offset: usize, nsects: usize) {
        debug_println!(
            "[   info]:{:>15}: text segment: iterating over {nsects} section(s) ...",
            "mach-o"
        );

        for sect_index in 0..nsects {
            let bindings::section_64 {
                flags,
                mut offset,
                size: sect_size,
                sectname,
                segname,
                ..
            } = self.container.read_type::<bindings::section_64>(
                offset
                    + size_of::<bindings::segment_command_64>()
                    + sect_index * size_of::<bindings::section_64>(),
            );

            let sect = CStr::from_bytes_until_nul(&sectname).unwrap_or_default();
            let seg = CStr::from_bytes_until_nul(&segname).unwrap_or_default();
            if (flags & bindings::SECTION_TYPE) == bindings::S_INIT_FUNC_OFFSETS {
                let func_end_offset = offset + sect_size as u32;
                while offset < func_end_offset {
                    debug_println!(
                        "[   info]:{:>15}: ({seg:?}/{sect:?}) S_INIT_FUNC_OFFSETS @ 0x{offset:x}",
                        "mach-o"
                    );

                    self.init_functions
                        .push(self.container.read_type::<u32>(offset as usize) as usize);

                    offset += size_of::<u32>() as u32;
                }
            } else {
                debug_println!(
                    "[warning]:{:>15}: ({seg:?}/{sect:?}) unsupported flags 0x{flags:x} @ 0x{offset:x}",
                    "mach-o"
                );
            }
        }
    }

    fn handle_data_segment(&mut self, offset: usize, nsects: usize) {
        debug_println!(
            "[   info]:{:>15}: data segment: iterating over {nsects} section(s) ...",
            "mach-o"
        );

        for sect_index in 0..nsects {
            let bindings::section_64 {
                flags,
                offset: mut start_off,
                size: sect_size,
                sectname,
                segname,
                ..
            } = self.container.read_type::<bindings::section_64>(
                offset
                    + size_of::<bindings::segment_command_64>()
                    + sect_index * size_of::<bindings::section_64>(),
            );

            let sect = CStr::from_bytes_until_nul(&sectname).unwrap_or_default();
            let seg = CStr::from_bytes_until_nul(&segname).unwrap_or_default();

            match flags & bindings::SECTION_TYPE {
                bindings::S_THREAD_LOCAL_VARIABLES => {
                    debug_println!(
                        "[   info]:{:>15}: ({seg:?}/{sect:?}) S_THREAD_LOCAL_VARIABLES @ 0x{offset:x}, startoff: 0x{start_off:x}, sect_size: 0x{sect_size}",
                        "mach-o"
                    );
                    self.thread_locals
                        .push((start_off as usize, ThreadLocalKind::Variable))
                }
                bindings::S_INIT_FUNC_OFFSETS => {
                    let func_end_offset = start_off + sect_size as u32;
                    while start_off < func_end_offset {
                        debug_println!(
                            "[   info]:{:>15}: ({seg:?}/{sect:?}) S_INIT_FUNC_OFFSETS @ 0x{offset:x}",
                            "mach-o"
                        );

                        self.init_functions
                            .push(self.container.read_type::<u32>(offset) as usize);

                        start_off += size_of::<u32>() as u32;
                    }
                }
                _ => {
                    debug_println!(
                        "[warning]:{:>15}: ({seg:?}/{sect:?}) unsupported flags 0x{flags:x} @ 0x{offset:x}",
                        "mach-o"
                    );
                }
            }
        }
    }

    fn handle_symtab(&mut self, offset: usize) {
        let bindings::symtab_command {
            symoff,
            nsyms,
            stroff,
            ..
        } = self.container.read_type(offset);

        let mut nlist_offset = symoff as usize;

        for _ in 0..nsyms {
            let bindings::nlist_64 {
                n_un,
                n_type,
                n_value,
                n_desc,
                n_sect,
            } = self.container.read_type(nlist_offset);

            if n_type & bindings::N_STAB != 0 {
                // don't care about debug symbols
                continue;
            }

            match n_type & bindings::N_TYPE {
                bindings::N_SECT => {
                    if (n_type & bindings::N_EXT) == 0 {
                        if n_desc & bindings::N_ALT_ENTRY != 0 {
                            if n_type & bindings::N_PEXT != 0 {
                                unimplemented!(
                                    "unsupported makeAltEntry with Scope::wasLinkageUnit"
                                );
                            } else {
                                unimplemented!("makeAltEntry with Scope::translationUnit");
                            }
                        } else if n_type & bindings::N_PEXT != 0 {
                            if n_desc & bindings::N_WEAK_DEF != 0 {
                                unimplemented!("makeWeakDefWasPrivateExtern");
                            } else {
                                //unimplemented!("makeRegularWasPrivateExtern");
                                self.symbols.push(Symbol::make_regular_export(
                                    self.container
                                        .read_str((stroff + n_un.n_strx) as usize)
                                        .to_string(),
                                    n_value as usize,
                                    n_sect,
                                    (n_desc & bindings::N_COLD_FUNC) != 0,
                                    (n_desc & bindings::N_ARM_THUMB_DEF) != 0,
                                ))
                            }
                        } else {
                            self.symbols.push(Symbol::make_regular_local(
                                self.container
                                    .read_str((stroff + n_un.n_strx) as usize)
                                    .to_string(),
                                n_value as usize,
                                n_sect,
                                (n_desc & bindings::N_COLD_FUNC) != 0,
                                (n_desc & bindings::N_ARM_THUMB_DEF) != 0,
                            ))
                        }
                    } else if n_type & bindings::N_PEXT != 0 {
                        if n_desc & bindings::N_ALT_ENTRY != 0 {
                            unimplemented!("makeAltEntry with Scope::linkageUnit");
                        } else if n_desc & bindings::N_WEAK_DEF != 0 {
                            unimplemented!("makeWeakDefHidden");
                        } else if n_desc & bindings::N_SYMBOL_RESOLVER != 0 {
                            unimplemented!("makeDynamicResolver with Scope::linkageUnit");
                        } else {
                            unimplemented!("makeRegularHidden");
                        }
                    } else if n_desc & bindings::N_ALT_ENTRY != 0 {
                        unimplemented!("makeAltEntry with Scope::global");
                    } else if (n_desc & (bindings::N_WEAK_DEF | bindings::N_WEAK_REF))
                        == (bindings::N_WEAK_DEF | bindings::N_WEAK_REF)
                    {
                        unimplemented!("makeWeakDefAutoHide");
                    } else if n_desc & bindings::N_WEAK_DEF != 0 {
                        self.symbols.push(Symbol::make_weak_def_export(
                            self.container
                                .read_str((stroff + n_un.n_strx) as usize)
                                .to_string(),
                            n_value as usize,
                            n_sect,
                            (n_desc & bindings::N_COLD_FUNC) != 0,
                            (n_desc & bindings::N_ARM_THUMB_DEF) != 0,
                        ))
                    } else if n_desc & bindings::N_SYMBOL_RESOLVER != 0 {
                        unimplemented!("makeDynamicResolver");
                    } else {
                        self.symbols.push(Symbol::make_regular_export(
                            self.container
                                .read_str((stroff + n_un.n_strx) as usize)
                                .to_string(),
                            n_value as usize,
                            n_sect,
                            (n_desc & bindings::N_COLD_FUNC) != 0,
                            (n_desc & bindings::N_ARM_THUMB_DEF) != 0,
                        ))
                    }
                }
                bindings::N_UNDF => {
                    if n_value == 0 {
                        self.symbols.push(Symbol::make_undefined(
                            self.container
                                .read_str((stroff + n_un.n_strx) as usize)
                                .to_string(),
                            get_library_ordinal(n_desc),
                            (n_desc & bindings::N_WEAK_REF) != 0,
                        ))
                    } else if n_type & bindings::N_PEXT != 0 {
                        unimplemented!("N_PEXT: n_type: {}", n_type & bindings::N_TYPE)
                    } else {
                        unimplemented!("n_type: {}", n_type & bindings::N_TYPE)
                    }
                }
                _ => println!(
                    "[warning]: unsupported n_type: {}",
                    n_type & bindings::N_TYPE
                ),
            }
            nlist_offset += size_of::<bindings::nlist_64>();
        }

        debug_println!(
            "[   info]:{:>15}: symtab_command @ 0x{offset:x}, {} symbol(s)",
            "mach-o",
            self.symbols.len()
        );
    }

    fn handle_rpath(&mut self, offset: usize) {
        println!("0x{:x}", offset + size_of::<load_command>());
        let rpath_data = self.container.read_cstr(offset + size_of::<load_command>());
    }

    fn handle_chained_fixups(&mut self, offset: usize) {
        let linkedit_data: bindings::linkedit_data_command = self.container.read_type(offset);
        let bindings::dyld_chained_fixups_header {
            imports_offset,
            starts_offset,
            symbols_offset,
            imports_format,
            ..
        } = self.container.read_type(linkedit_data.dataoff as usize);

        let fixup_data_offset = linkedit_data.dataoff;

        let starts_offset = fixup_data_offset + starts_offset;
        let starts_info = self
            .container
            .read_type::<bindings::dyld_chained_starts_in_image>(starts_offset as usize);

        let segment_offsets = self.container.read_array::<u32>(
            starts_offset + size_of::<bindings::dyld_chained_starts_in_image>() as u32,
            starts_info.seg_count as usize,
        );

        let imports_offset = fixup_data_offset + imports_offset;
        let symbols_offset = fixup_data_offset + symbols_offset;

        for seg_offset in &segment_offsets {
            if *seg_offset == 0 {
                continue;
            }

            let seg_starts_offset = starts_offset + seg_offset;
            let bindings::dyld_chained_starts_in_segment {
                page_count,
                segment_offset,
                page_size,
                pointer_format,
                ..
            } = self.container.read_type(seg_starts_offset as usize);

            const PAGE_STARTS_OFFSET: u32 = 22;
            let page_starts = self
                .container
                .read_array::<u16>(seg_starts_offset + PAGE_STARTS_OFFSET, page_count as usize);

            for (page_index, &page_start) in page_starts.iter().enumerate() {
                let mut chain_offset = segment_offset as usize
                    + (page_index * page_size as usize)
                    + page_start as usize;

                loop {
                    let fixup_value = self.container.read_type::<u64>(chain_offset);
                    let (next_offset, fixup) = match pointer_format {
                        bindings::dyld_chained_ptr_format_variants::DYLD_CHAINED_PTR_64
                        | bindings::dyld_chained_ptr_format_variants::DYLD_CHAINED_PTR_64_OFFSET => self
                            .parse_64bit_fixup(
                                fixup_value,
                                chain_offset,
                                imports_format,
                                            imports_offset as u64,
                            symbols_offset as u64,
                            ),
                        bindings::dyld_chained_ptr_format_variants::DYLD_CHAINED_PTR_ARM64E
                        | bindings::dyld_chained_ptr_format_variants::DYLD_CHAINED_PTR_ARM64E_USERLAND => {
                            self.parse_arm64e_fixup(
                                fixup_value,
                                chain_offset,
                                imports_format,
                                            imports_offset as u64,
                            symbols_offset as u64,
                            )
                        }
                        bindings::dyld_chained_ptr_format_variants::DYLD_CHAINED_PTR_ARM64E_USERLAND24 => {
                            self.parse_arm64e_userland24_fixup(
                                fixup_value,
                                chain_offset,
                                imports_format,
                                            imports_offset as u64,
                            symbols_offset as u64,
                            )
                        }
                        _ => unimplemented!("pointer format: {pointer_format:?}"),
                    };

                    self.fixups.push(fixup);

                    if next_offset == 0 {
                        break;
                    }

                    chain_offset += next_offset as usize;
                }
            }
        }

        debug_println!(
            "[   info]:{:>15}: linkedit_data_command @ 0x{offset:x}, {} fixup(s)",
            "mach-o",
            self.fixups.len()
        );
    }

    fn parse_64bit_fixup(
        &self,
        value: u64,
        offset: usize,
        imports_format: bindings::dyld_chained_import_format_variants,
        imports_offset: u64,
        symbols_offset: u64,
    ) -> (u64, ChainedFixup) {
        let bind = (value >> 63) & 1;

        if bind == 1 {
            let bind_ptr = bindings::dyld_chained_ptr_64_bind(value);
            let (lib_ordinal, symbol_name) = self.read_import_symbol(
                bind_ptr.ordinal(),
                imports_format,
                imports_offset,
                symbols_offset,
            );

            (
                bind_ptr.next() as u64 * 4,
                ChainedFixup {
                    offset,
                    kind: FixupKind::Bind {
                        symbol_name,
                        is_auth: false,
                        ordinal: lib_ordinal as u32,
                        addend: bind_ptr.addend() as i64,
                    },
                },
            )
        } else {
            let rebase_ptr = bindings::dyld_chained_ptr_64_rebase(value);
            let target = rebase_ptr.target();
            let high8 = (rebase_ptr.high8() as u64) << 56;
            let next = rebase_ptr.next() as u64 * 4;

            (
                next,
                ChainedFixup {
                    offset,
                    kind: FixupKind::Rebase {
                        target: target | high8,
                    },
                },
            )
        }
    }

    fn parse_arm64e_fixup(
        &self,
        value: u64,
        offset: usize,
        imports_format: bindings::dyld_chained_import_format_variants,
        imports_offset: u64,
        symbols_offset: u64,
    ) -> (u64, ChainedFixup) {
        let bind = (value >> 62) & 1;
        let auth = (value >> 63) & 1;

        if bind == 1 {
            if auth == 1 {
                let auth_bind = bindings::dyld_chained_ptr_arm64e_auth_bind(value);
                let (lib_ordinal, symbol_name) = self.read_import_symbol(
                    auth_bind.ordinal() as u32,
                    imports_format,
                    imports_offset,
                    symbols_offset,
                );

                (
                    auth_bind.next() as u64 * 8,
                    ChainedFixup::new_bind(offset, symbol_name, lib_ordinal as u32, 0, true),
                )
            } else {
                let bind_ptr = bindings::dyld_chained_ptr_arm64e_bind(value);
                let (lib_ordinal, symbol_name) = self.read_import_symbol(
                    bind_ptr.ordinal() as u32,
                    imports_format,
                    imports_offset,
                    symbols_offset,
                );

                (
                    bind_ptr.next() as u64 * 8,
                    ChainedFixup::new_bind(
                        offset,
                        symbol_name,
                        lib_ordinal as u32,
                        bind_ptr.addend() as i64,
                        false,
                    ),
                )
            }
        } else if auth == 1 {
            let auth_rebase = bindings::dyld_chained_ptr_arm64e_auth_rebase(value);

            (
                auth_rebase.next() as u64 * 8,
                ChainedFixup::new_rebase(offset, auth_rebase.target() as u64),
            )
        } else {
            let rebase = bindings::dyld_chained_ptr_arm64e_rebase(value);
            let high8 = (rebase.high8() as u64) << 56;

            (
                rebase.next() as u64 * 8,
                ChainedFixup::new_rebase(offset, rebase.target() | high8),
            )
        }
    }

    fn parse_arm64e_userland24_fixup(
        &self,
        value: u64,
        offset: usize,
        imports_format: bindings::dyld_chained_import_format_variants,
        imports_offset: u64,
        symbols_offset: u64,
    ) -> (u64, ChainedFixup) {
        let bind = (value >> 62) & 1;
        let auth = (value >> 63) & 1;

        if bind == 1 {
            if auth == 1 {
                let auth_bind = bindings::dyld_chained_ptr_arm64e_auth_bind_24(value);
                let (lib_ordinal, symbol_name) = self.read_import_symbol(
                    auth_bind.ordinal(),
                    imports_format,
                    imports_offset,
                    symbols_offset,
                );
                (
                    auth_bind.next() as u64 * 8,
                    ChainedFixup::new_bind(offset, symbol_name, lib_ordinal as u32, 0, true),
                )
            } else {
                let bind = bindings::dyld_chained_ptr_arm64e_bind_24(value);
                let (lib_ordinal, symbol_name) = self.read_import_symbol(
                    bind.ordinal(),
                    imports_format,
                    imports_offset,
                    symbols_offset,
                );

                (
                    bind.next() as u64 * 8,
                    ChainedFixup::new_bind(
                        offset,
                        symbol_name,
                        lib_ordinal as u32,
                        bind.addend() as i64,
                        false,
                    ),
                )
            }
        } else {
            self.parse_arm64e_fixup(
                value,
                offset,
                imports_format,
                imports_offset,
                symbols_offset,
            )
        }
    }

    fn read_import_symbol(
        &self,
        ordinal: u32,
        imports_format: bindings::dyld_chained_import_format_variants,
        imports_offset: u64,
        symbols_offset: u64,
    ) -> (u8, String) {
        let (lib_ordinal, name_offset) = match imports_format {
            bindings::dyld_chained_import_format_variants::DYLD_CHAINED_IMPORT => {
                let import = self.container.read_type::<bindings::dyld_chained_import>(
                    imports_offset as usize + (ordinal * 4) as usize,
                );

                (import.lib_ordinal(), import.name_offset())
            }
            bindings::dyld_chained_import_format_variants::DYLD_CHAINED_IMPORT_ADDEND => {
                let import = self
                    .container
                    .read_type::<bindings::dyld_chained_import_addend>(
                        imports_offset as usize + (ordinal * 8) as usize,
                    );

                (import.header.lib_ordinal(), import.addend as u32)
            }
            bindings::dyld_chained_import_format_variants::DYLD_CHAINED_IMPORT_ADDEND64 => {
                let import = self
                    .container
                    .read_type::<bindings::dyld_chained_import_addend64>(
                        imports_offset as usize + (ordinal * 16) as usize,
                    );

                (import.header.lib_ordinal(), import.addend as u32)
            }
        };

        (
            lib_ordinal,
            self.container
                .read_str((symbols_offset + name_offset as u64) as usize)
                .to_string(),
        )
    }
}

fn create_arch_container<'a>(container: &Container<'a>, index: u32) -> Option<Container<'a>> {
    let offset =
        size_of::<bindings::fat_header>() + index as usize * size_of::<bindings::fat_arch>();

    let bindings::fat_arch { offset, size, .. } = container.read_type::<bindings::fat_arch>(offset);

    container
        .slice(offset as usize, size as usize)
        .map(Container::with_bytes)
}

fn is_of_arch(container: &Container<'_>, arch: i32) -> bool {
    let header = container.read_type::<bindings::mach_header_64>(0);
    header.magic == bindings::mach_magic::MH_MAGIC_64 && header.cputype == arch
}
