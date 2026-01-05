use std::ptr::NonNull;

#[allow(clippy::missing_safety_doc)]
pub unsafe fn entry(
    entrypoint: NonNull<u8>,
    argc: usize,
    argv: *const *const u8,
    envp: *const *const u8,
) -> ! {
    unsafe { entry_and_ret(entrypoint, argc, argv, envp) };
    std::process::exit(0)
}

#[allow(clippy::missing_safety_doc)]
pub unsafe fn entry_and_ret(
    entrypoint: NonNull<u8>,
    argc: usize,
    argv: *const *const u8,
    envp: *const *const u8,
) {
    //let envp = unsafe { *libc::_NSGetEnviron() } as *const *const u8;

    let entry_fn = unsafe {
        std::mem::transmute::<*mut u8, extern "C" fn(usize, *const *const u8, *const *const u8)>(
            entrypoint.as_ptr(),
        )
    };

    entry_fn(argc, argv, envp);
}
