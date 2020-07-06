use libc::c_char;
use std::ffi::CString;
use std::mem;

/// Converts the string into a C-compatible null terminated string,
/// then forgets the container while returning a pointer to the
/// underlying buffer.
///
/// The returned pointer must be passed to CString::from_raw to
/// prevent leaking memory.
pub(crate) fn into_leaked_cstring<T>(
    t: T,
) -> Result<*const c_char, anyhow::Error>
where
    T: Into<Vec<u8>>,
{
    let copy = CString::new(t)?;
    let ptr = copy.as_ptr();

    // Intentionally leak this memory so that it stays
    // valid while C is using it.
    mem::forget(copy);

    Ok(ptr)
}

/// Delete a string previously returned by this FFI.
///
/// It is explicitly allowed to pass a null pointer to this function;
/// in that case the function will do nothing.
#[no_mangle]
pub extern "C" fn string_delete(string: *mut c_char) {
    ffi! {
        name: "string_delete",
        params: [string],
        op: {
            if string.is_null() {
                return Ok(());
            }

            let string = unsafe { CString::from_raw(string) };
            std::mem::drop(string);
            Ok(())
        },
        fail: {
        }
    }
}
