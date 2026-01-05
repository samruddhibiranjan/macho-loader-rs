#[allow(non_camel_case_types)]
pub mod bindings;

pub mod container;

pub fn name_eq<T: AsRef<[u8]>>(segname: &[u8; 16], name: T) -> bool {
    segname.as_ref().starts_with(name.as_ref())
}
