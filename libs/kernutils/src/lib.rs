use libc::{KERN_SUCCESS, kern_return_t, mach_error_string, mach_port_t, task_t};

/// Aborts by panicking with a message
///
/// This function translates the kern_return into a human
/// readable string and panics while displaying it.
///
/// This should be used when a kernel api function fails in a
/// way that we don't want to recover
fn panic_kern_err(kern_return: kern_return_t) -> ! {
    let msg = unsafe {
        let ptr = mach_error_string(kern_return);
        if ptr.is_null() {
            panic!("panic_kern_err: {kern_return}: no error message");
        }

        std::ffi::CStr::from_ptr(ptr)
            .to_str()
            .unwrap_or("<invalid utf8>")
    };

    panic!("panic_kern_err: {kern_return}: {msg}");
}

unsafe extern "C" {
    /// the caller's mach_task_self_ environment variable
    unsafe static mach_task_self_: mach_port_t;

    /// return a task port to the process `pid`.
    /// various security mechanisms are in place to control
    /// what ports can be aquired, some entitlements might have to be
    /// enabled.
    pub unsafe fn _task_for_pid(
        target_tport: task_t,
        pid: libc::c_int,
        tn: *mut task_t,
    ) -> kern_return_t;

    /// The 64 bit version of the function function `vm_allocate`.
    /// Allocates a region of virtual memory, placing it in the specified
    /// task's address space.
    /// The starting address is address. If the anywhere option is false,
    /// an attempt is made to allocate virtual memory starting at this virtual address.
    /// If this address is not at the beginning of a virtual page, it will be rounded down to one.
    /// If there is not enough space at this address, no memory will be allocated.
    /// If the anywhere option is true, the input value of this address will be ignored,
    /// and the space will be allocated wherever it is available.
    /// In either case, the address at which memory was actually allocated will be returned in address.
    /// size is the number of bytes to allocate (rounded by the system in a machine
    /// dependent way to an integral number of virtual pages). If anywhere is true, the kernel
    /// should find and allocate any region of the specified size, and return the address of
    /// the resulting region in address address, rounded to a virtual page boundary
    /// if there is sufficient space.
    ///
    /// The physical memory is not actually allocated until the new virtual memory is referenced.
    /// By default, the kernel rounds all addresses down to the nearest page boundary and all
    /// memory sizes up to the nearest page size. The global variable vm_page_size contains
    /// the page size. mach_task_self returns the value of the current task port which
    /// should be used as the target_task argument in order to allocate memory in the
    /// caller's address space. For languages other than C, these values can be obtained
    /// by the calls vm_statistics and mach_task_self. Initially, the pages of allocated memory
    /// will be protected to allow all forms of access, and will be inherited in child tasks
    /// as a copy.
    /// Subsequent calls to vm_protect and vm_inherit may be used to change these properties.
    /// The allocated region is always zero-filled.
    ///
    /// The function returns KERN_SUCCESS if the memory was successfully allocated, KERN_INVALID_ADDRESS
    /// if an invalid address was specified and KERN_NO_SPACE if there was not enough space
    /// left to satisfy the request.
    pub unsafe fn mach_vm_allocate(
        target: task_t,
        address: *mut libc::mach_vm_address_t,
        size: libc::mach_vm_size_t,
        flags: libc::c_int,
    ) -> kern_return_t;

    /// The vm_deallocate function deallocates a region of virtual memory in the specified
    /// task's address space.
    /// The region starts at the beginning of the virtual page containing address and ends
    /// at the end of the virtual page containing address + size - 1.
    /// Because of this rounding to virtual page boundaries, the amount of memory deallocated
    /// may be greater than size.
    /// Use host_page_size to find the current virtual page size.
    pub unsafe fn mach_vm_deallocate(
        target: task_t,
        address: libc::mach_vm_address_t,
        size: libc::mach_vm_size_t,
    ) -> kern_return_t;

    ///  The vm_write function writes an array of data to a task's virtual memory.
    /// It allows one task to write to another task's memory. The result of vm_write is as
    /// if target_task had directly written into the set of pages.
    /// Hence, target_task must have write permission to the pages.
    pub unsafe fn mach_vm_write(
        target: task_t,
        address: libc::mach_vm_address_t,
        data_u: libc::mach_vm_address_t,
        size: libc::mach_vm_size_t,
    ) -> kern_return_t;

    /// The vm_protect function sets access privileges for a region within the specified task's
    /// address space.
    /// The new_protection parameter specifies a combination of read, write, and execute accesses
    /// that are allowed (rather than prohibited).
    /// The region starts at the beginning of the virtual page containing address; it ends at
    /// the end of the virtual page containing address + size - 1.
    /// Because of this rounding to virtual page boundaries, the amount of memory protected may
    /// be greater than size.
    /// Use host_page_size to find the current virtual page size.
    /// The enforcement of virtual memory protection is machine-dependent.
    /// Nominally read access requires VM_PROT_READ permission, write access requires VM_PROT_WRITE
    /// permission, and execute access requires VM_PROT_EXECUTE permission.
    /// However, some combinations of access rights may not be supported.
    /// In particular, the kernel interface allows write access to require VM_PROT_READ and
    /// VM_PROT_WRITE permission and execute access to require VM_PROT_READ permission.
    pub unsafe fn mach_vm_protect(
        task: task_t,
        address: libc::mach_vm_address_t,
        size: libc::mach_vm_size_t,
        set_maximum: libc::boolean_t,
        new_protection: libc::vm_prot_t,
    ) -> kern_return_t;
}

/// Returns a task port for the current task (the caller).
///
/// This port is used to make further calls to the mach_* api functions.
/// eg. when we need to allocate memory for the loaded programs, we
/// first get this port, then call the memory allocation functions.
#[inline]
fn mach_task_self() -> mach_port_t {
    unsafe { mach_task_self_ }
}

/// vm allocation flag
const VM_FLAGS_ANYWHERE: i32 = 0x00000001;

/// Deallocates memory for the current task.
///
/// This functions is a wrapper around `mach_vm_deallocate` passing it
/// the current tasks port.
#[inline]
pub fn vm_alloc_task_self(size: usize) -> std::ptr::NonNull<u8> {
    unsafe { vm_alloc_internal(size).unwrap_or_else(|kern_error| panic_kern_err(kern_error)) }
}

#[inline]
pub fn vm_dealloc_task_self(address: libc::mach_vm_address_t, size: usize) {
    unsafe {
        vm_dealloc_internal(address, size as libc::vm_size_t)
            .unwrap_or_else(|kern_error| panic_kern_err(kern_error))
    }
}

/// Copies memory (overwrite old) into the current tasks
/// address space.
///
/// This function takes the address of where to write the data,
/// and the address of the source data to write. Note that the
/// destination should be some address in the current tasks
/// writable memory. The writing will fail if the memory was
/// not properly protected beforehand.
#[inline]
pub fn vm_copy_into_task_self(
    src: libc::mach_vm_address_t,
    dst: libc::mach_vm_address_t,
    count: usize,
) {
    unsafe {
        vm_copy_overwrite_internal(src, dst, count)
            .unwrap_or_else(|kern_error| panic_kern_err(kern_error))
    }
}

/// Applies protections to an address range on the current task.
///
/// This function can be called to change the protection of some
/// mapped memory. A common use case if to enable write protection
/// before writing and reapplying the initial protection afterwards.
#[inline]
pub fn vm_protect(
    ptr: libc::mach_vm_address_t,
    size: usize,
    set_maximum: libc::boolean_t,
    protection: libc::vm_prot_t,
) {
    unsafe {
        vm_protect_internal(ptr, size as u64, set_maximum, protection)
            .unwrap_or_else(|kern_error| panic_kern_err(kern_error))
    }
}

#[inline]
unsafe fn vm_alloc_internal(size: usize) -> Result<std::ptr::NonNull<u8>, kern_return_t> {
    let mut addr = 0;
    let kern_return = unsafe {
        mach_vm_allocate(
            mach_task_self(),
            &mut addr,
            size as libc::mach_vm_size_t,
            VM_FLAGS_ANYWHERE,
        )
    };
    match kern_return {
        KERN_SUCCESS => {
            std::ptr::NonNull::new(addr as *mut u8).ok_or_else(|| panic!("ptr is null"))
        }
        _ => Err(kern_return),
    }
}

#[inline]
unsafe fn vm_dealloc_internal(
    address: libc::mach_vm_address_t,
    size: usize,
) -> Result<(), kern_return_t> {
    let kern_return =
        unsafe { mach_vm_deallocate(mach_task_self(), address, size as libc::mach_vm_size_t) };

    match kern_return {
        KERN_SUCCESS => Ok(()),
        _ => Err(kern_return),
    }
}

#[inline]
unsafe fn vm_copy_overwrite_internal(
    src: libc::mach_vm_address_t,
    dst: libc::mach_vm_address_t,
    count: usize,
) -> Result<(), kern_return_t> {
    let kern_return =
        unsafe { mach_vm_write(mach_task_self(), dst, src, count as libc::mach_vm_size_t) };

    match kern_return {
        KERN_SUCCESS => Ok(()),
        _ => Err(kern_return),
    }
}

#[inline]
unsafe fn vm_protect_internal(
    ptr: libc::mach_vm_address_t,
    size: libc::mach_vm_size_t,
    set_maximum: libc::boolean_t,
    protection: libc::vm_prot_t,
) -> Result<(), kern_return_t> {
    unsafe {
        let kern_return = mach_vm_protect(mach_task_self(), ptr, size, set_maximum, protection);
        match kern_return {
            KERN_SUCCESS => Ok(()),
            kern_return => Err(kern_return),
        }
    }
}
