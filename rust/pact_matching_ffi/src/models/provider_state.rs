//! The `ProviderState` type and operations on it.

use crate::ffi;
use crate::util::string;
use libc::c_char;
use std::ffi::CString;

pub use pact_matching::models::provider_states::ProviderState as NonCProviderState;

/// FFI structure mirroring the internal Rust ProviderState struct.
/// Contains the name of this Provider State,
/// and a list of (key, value) parameters as an array of structures.
/// The number of elements is stored in 'params_length'.
///
/// This structure should not be mutated.
#[allow(missing_copy_implementations)]
#[repr(C)]
#[derive(Debug)]
pub struct ProviderState {
    /// null terminated string containing the name
    pub name: *const c_char,
    /// pointer to array of key, value pairs
    pub params_list: *const ProviderStateParamsKV,
    /// number of elements in `params_list`
    pub params_length: usize,
    /// private, tracks allocated capacity of the underlying Vec
    capacity: usize,
}

/// FFI structure representing a (key, value) pair
/// for the ProviderState parameters.
///
/// The `value` field is a JSON object, serialized to a string.
///
/// This structure should not be mutated.
#[allow(missing_copy_implementations)]
#[repr(C)]
#[derive(Debug)]
pub struct ProviderStateParamsKV {
    /// null terminated string containing the key
    pub key: *const c_char,
    /// null terminated JSON string
    pub value: *const c_char,
}

/// Delete a ProviderState previously returned by this FFI.
///
/// It is explicitly allowed to pass a null pointer to this function;
/// in that case the function will do nothing.
#[no_mangle]
pub extern "C" fn provider_state_delete(
    provider_state: *const ProviderState,
) {
    ffi! {
        name: "provider_state_delete",
        params: [provider_state],
        op: {
            if provider_state.is_null() {
                return Ok(());
            }

            impl_provider_state_delete(provider_state);

            Ok(())
        },
        fail: {
        }
    }
}

/// Create and leak a ProviderState.  Must be passed back to
/// impl_provider_state_delete to clean up memory.
pub(crate) fn into_leaked_provider_state(
    provider_state: &NonCProviderState,
) -> Result<*const ProviderState, anyhow::Error> {
    let name = &provider_state.name;
    let params = &provider_state.params;
    let mut list = Vec::with_capacity(params.len());

    // First check all the strings for embedded null.
    // This prevents leaking memory in the case where
    // an error occurs after some strings were intentionally
    // leaked, but before they can be passed to C.

    if name.find(|c| c == '\0').is_some() {
        anyhow::bail!(
            "Found embedded null in \
                      a provider state name: '{}'",
            name
        );
    }

    for (k, _v) in params.iter() {
        if k.find(|c| c == '\0').is_some() {
            anyhow::bail!(
                "Found embedded null in \
                          a provider state key name: '{}'",
                k
            );
        }
    }

    for (k, v) in params.iter() {
        // It is safe to unwrap, since the strings were already
        // checked for embedded nulls.
        let kv = ProviderStateParamsKV {
            key: string::into_leaked_cstring(k.clone()).unwrap(),
            value: string::into_leaked_cstring(v.to_string()).unwrap(),
        };

        list.push(kv);
    }

    let provider_state_ffi = ProviderState {
        // It is safe to unwrap, since the string was already
        // checked for embedded nulls.
        name: string::into_leaked_cstring(name.clone()).unwrap(),
        params_list: list.as_ptr(),
        params_length: list.len(),
        capacity: list.capacity(),
    };

    std::mem::forget(list);

    let output = Box::new(provider_state_ffi);

    Ok(Box::into_raw(output))
}

/// Manually delete a ProviderState.
/// Returns all leaked memory into Rust structures, which will
/// be automatically cleaned up on Drop.
fn impl_provider_state_delete(ptr: *const ProviderState) {
    let provider_state =
        unsafe { Box::from_raw(ptr as *mut ProviderState) };

    let _name =
        unsafe { CString::from_raw(provider_state.name as *mut c_char) };

    let list = unsafe {
        Vec::from_raw_parts(
            provider_state.params_list as *mut ProviderStateParamsKV,
            provider_state.params_length,
            provider_state.capacity,
        )
    };

    for kv in list {
        let _k = unsafe { CString::from_raw(kv.key as *mut c_char) };
        let _v = unsafe { CString::from_raw(kv.value as *mut c_char) };
    }
}
