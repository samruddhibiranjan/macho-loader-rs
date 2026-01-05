use scroll::{BE, LE, Pread, ctx::TryFromCtx};

use crate::bindings::mach_magic;

/// The endianness of a MachO binary
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endian {
    Little,
    Big,
}

impl Endian {
    /// Returns a Endian given a magic number.
    ///
    /// Returns None if the magic number is unknown.
    pub fn from_magic(magic: mach_magic) -> Self {
        match magic {
            mach_magic::MH_MAGIC | mach_magic::MH_MAGIC_64 => Self::Little,
            mach_magic::MH_CIGAM
            | mach_magic::MH_CIGAM_64
            | mach_magic::FAT_MAGIC
            | mach_magic::FAT_MAGIC_64
            | mach_magic::FAT_CIGAM
            | mach_magic::FAT_CIGAM_64 => Self::Big,
        }
    }
}

impl From<Endian> for scroll::Endian {
    fn from(e: Endian) -> Self {
        match e {
            Endian::Little => LE,
            Endian::Big => BE,
        }
    }
}

/// Container for raw Mach-O binary data
///
/// This is used to help reading data from the binary.
/// Every read from the image should be done through this container
/// so data gets returned in a meaningful way.
#[derive(Debug, Clone, Copy)]
pub struct Container<'bytes> {
    /// The inner MachO bytes
    bytes: &'bytes [u8],

    /// The endianness of the inner binary
    pub endian: Endian,
}

impl<'bytes> Container<'bytes> {
    /// Creates a new container that wraps a given macho.
    ///
    /// It attempts to parse the magic number and determine endianness.
    /// This function panics if the first bytes don't make sense.
    pub fn with_bytes(bytes: &'bytes [u8]) -> Self {
        let magic: mach_magic = bytes
            .pread(0)
            .unwrap_or_else(|err| panic!("pread error: {err}"));

        Self {
            bytes,
            endian: Endian::from_magic(magic),
        }
    }

    /// Reads a type from the inner Macho.
    ///
    /// This function takes an offset and tries to serialize
    /// a value of the type provided (T), from the inner container.
    ///
    /// This is useful for reading values when endianness can vary
    ///
    /// the type must derive [`Pread`].
    #[inline]
    pub fn read_type<T>(&self, offset: usize) -> T
    where
        T: TryFromCtx<'bytes, scroll::Endian, Error = scroll::Error>,
    {
        self.bytes
            .pread_with(offset, scroll::Endian::from(self.endian))
            .unwrap_or_else(|err| panic!("pread error: {err}"))
    }

    /// Read a contiguous array of types from the inner Macho
    ///
    /// This function calls `read_type` in a loop for a given type.
    pub fn read_array<T>(&self, offset: u32, count: usize) -> Vec<T>
    where
        T: TryFromCtx<'bytes, scroll::Endian, Error = scroll::Error>,
    {
        (0..count)
            .map(|i| self.read_type::<T>((offset + (i * size_of::<T>()) as u32) as usize))
            .collect()
    }

    /// Returns some inner bytes given a range
    #[inline]
    pub fn slice(&self, offset: usize, len: usize) -> Option<&'bytes [u8]> {
        self.bytes.get(offset..offset.checked_add(len)?)
    }

    /// returns the inner bytes
    #[inline]
    pub fn as_bytes(&self) -> &'bytes [u8] {
        self.bytes
    }

    /// Reads a C string from the container at the given offset.
    ///
    /// Returns a string slice up to (but not including) the null terminator.
    /// This function panics if the offset is out of bounds or if no null
    /// terminator is found.
    pub fn read_str(&self, offset: usize) -> &'bytes str {
        let bytes_from_offset = self
            .bytes
            .get(offset..)
            .unwrap_or_else(|| panic!("out of bounds"));

        let null_pos = bytes_from_offset
            .iter()
            .position(|&b| b == 0)
            .unwrap_or_else(|| panic!("no null terminator"));

        let cstring_bytes = &bytes_from_offset[..null_pos];

        std::str::from_utf8(cstring_bytes).unwrap_or_else(|err| panic!("invalid UTF-8: {err}"))
    }

    /// Reads a null-terminated C string from the container at the given offset,
    /// returning it as a CStr.
    ///
    /// This includes the null terminator in the returned CStr.
    /// This function panics if the offset is out of bounds or if no null
    /// terminator is found.
    pub fn read_cstr(&self, offset: usize) -> &'bytes std::ffi::CStr {
        let bytes_from_offset = self
            .bytes
            .get(offset..)
            .unwrap_or_else(|| panic!("out of bounds"));

        let null_pos = bytes_from_offset
            .iter()
            .position(|&b| b == 0)
            .unwrap_or_else(|| panic!("no null terminator"));

        let cstr_bytes = &bytes_from_offset[..=null_pos];

        std::ffi::CStr::from_bytes_with_nul(cstr_bytes)
            .unwrap_or_else(|err| panic!("invalid CStr: {err}"))
    }
}
