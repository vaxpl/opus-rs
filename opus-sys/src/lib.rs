#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(clippy::op_ref)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        let cstr = unsafe { std::ffi::CStr::from_ptr(opus_get_version_string()) };
        assert_eq!(cstr.to_str(), Ok("libopus 1.3.1"));
    }
}
