//! The Pact `Message` type, including associated matching rules and provider states.

/*===============================================================================================
 * # Imports
 *---------------------------------------------------------------------------------------------*/

use crate::ffi;
use crate::models::pact_specification::PactSpecification;
use crate::models::provider_state::{
    into_leaked_provider_state, ProviderState,
};
use crate::util::*;
use anyhow::{anyhow, Context};
use libc::{c_char, c_int, c_uint, EXIT_FAILURE, EXIT_SUCCESS};
use std::ffi::CStr;

/*===============================================================================================
 * # Re-Exports
 *---------------------------------------------------------------------------------------------*/

// Necessary to make 'cbindgen' generate an opaque struct on the C side.
pub use pact_matching::models::message::Message;

/*===============================================================================================
 * # FFI Functions
 *---------------------------------------------------------------------------------------------*/

/// Get a mutable pointer to a newly-created default message on the heap.
#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn message_new() -> *mut Message {
    ffi! {
        name: "message_new",
        params: [],
        op: {
            Ok(ptr::raw_to(Message::default()))
        },
        fail: {
            ptr::null_mut_to::<Message>()
        }
    }
}

/// Constructs a `Message` from the JSON string
#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn message_new_from_json(
    index: c_uint,
    json_str: *const c_char,
    spec_version: PactSpecification,
) -> *mut Message {
    ffi! {
        name: "message_new_from_json",
        params: [index, json_str, spec_version],
        op: {
            if json_str.is_null() {
                anyhow::bail!("json_str is null");
            }

            let json_str = CStr::from_ptr(json_str);
            let json_str = json_str
                .to_str()
                .context("error parsing json_str as UTF-8")?;

            let json_value: serde_json::Value =
                serde_json::from_str(json_str)
                .context("error parsing json_str as JSON")?;

            let message = Message::from_json(
                index as usize,
                &json_value,
                &spec_version.into())
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            Ok(ptr::raw_to(message))
        },
        fail: {
            ptr::null_mut_to::<Message>()
        }
    }
}

/// Destroy the `Message` being pointed to.
#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn message_delete(message: *mut Message) -> c_int {
    ffi! {
        name: "message_delete",
        params: [message],
        op: {
            ptr::drop_raw(message);
            Ok(EXIT_SUCCESS)
        },
        fail: {
            EXIT_FAILURE
        }
    }
}

/// Get a copy of the description.
/// The returned string must be deleted with `string_delete`.
///
/// Since it is a copy, the returned string may safely outlive
/// the `Message`.
///
/// # Errors
///
/// On failure, this function will return a NULL pointer.
///
/// This function may fail if the Rust string contains embedded
/// null ('\0') bytes.
#[no_mangle]
#[allow(clippy::missing_safety_doc)]
#[allow(clippy::or_fun_call)]
pub unsafe extern "C" fn message_get_description(
    message: *const Message,
) -> *const c_char {
    ffi! {
        name: "message_get_description",
        params: [message],
        op: {
            let message = message.as_ref().ok_or(anyhow!("message is null"))?;
            let description = string::into_leaked_cstring(message.description.clone())?;
            Ok(description)
        },
        fail: {
            ptr::null_to::<c_char>()
        }
    }
}

/// Write the `description` field on the `Message`.
///
/// `description` must contain valid UTF-8. Invalid UTF-8
/// will be replaced with U+FFFD REPLACEMENT CHARACTER.
///
/// This function will only reallocate if the new string
/// does not fit in the existing buffer.
#[no_mangle]
#[allow(clippy::missing_safety_doc)]
#[allow(clippy::or_fun_call)]
pub unsafe extern "C" fn message_set_description(
    message: *mut Message,
    description: *const c_char,
) -> c_int {
    ffi! {
        name: "message_set_description",
        params: [message, description],
        op: {
            let message = message.as_mut().ok_or(anyhow!("message is null"))?;

            if description.is_null() {
                anyhow::bail!("description is null");
            }

            let description = CStr::from_ptr(description);

            // Get an owned Rust `String` from `description`.
            let description = description.to_str()
                                         .map_err(|e| {
                                             let last_valid_byte = e.valid_up_to();
                                             anyhow!("description isn't valid UTF-8 (valid up to byte {})", last_valid_byte)
                                         })?
                                         .to_owned();

            // Wipe out the previous contents of the string, without
            // deallocating, and set the new description.
            message.description.clear();
            message.description.push_str(&description);

            Ok(EXIT_SUCCESS)
        },
        fail: {
            EXIT_FAILURE
        }
    }
}

/// Get a copy of the provider state at the given index from this message.
/// A pointer to the structure will be written to `out_provider_state`,
/// only if no errors are encountered.
///
/// The returned structure must be deleted with `provider_state_delete`.
///
/// Since it is a copy, the returned structure may safely outlive
/// the `Message`.
///
/// # Errors
///
/// On failure, this function will return a variant other than Success.
///
/// This function may fail if the index requested is out of bounds,
/// or if any of the Rust strings contain embedded null ('\0') bytes.
#[no_mangle]
#[allow(clippy::missing_safety_doc)]
#[allow(clippy::or_fun_call)]
pub unsafe extern "C" fn message_get_provider_state(
    message: *const Message,
    index: usize,
    out_provider_state: *mut *const ProviderState,
) -> c_int {
    ffi! {
        name: "message_get_provider_state",
        params: [message, index, out_provider_state],
        op: {
            let message = message.as_ref().ok_or(anyhow!("message is null"))?;

            let provider_state = message.provider_states.get(index).ok_or(anyhow!("index is out of bounds"))?;

            std::ptr::write(out_provider_state, into_leaked_provider_state(provider_state)?);

            Ok(EXIT_SUCCESS)
        },
        fail: {
            EXIT_FAILURE
        }
    }
}

/// Get a copy of the metadata value indexed by `key`.
/// The returned string must be deleted with `string_delete`.
///
/// Since it is a copy, the returned string may safely outlive
/// the `Message`.
///
/// The returned pointer will be NULL if the metadata does not contain
/// the given key, or if an error occurred.
///
/// # Errors
///
/// On failure, this function will return a NULL pointer.
///
/// This function may fail if the provided `key` string contains
/// invalid UTF-8, or if the Rust string contains embedded null ('\0')
/// bytes.
#[no_mangle]
#[allow(clippy::missing_safety_doc)]
#[allow(clippy::or_fun_call)]
pub unsafe extern "C" fn message_find_metadata(
    message: *const Message,
    key: *const c_char,
) -> *const c_char {
    ffi! {
        name: "message_find_metadata",
        params: [message, key],
        op: {
            let message = message.as_ref().ok_or(anyhow!("message is null"))?;

            if key.is_null() {
                anyhow::bail!("key is null");
            }

            let key = CStr::from_ptr(key);
            let key = key
                .to_str()
                .context("error parsing key as UTF-8")?;

            match message.metadata.get(key) {
                None => Ok(ptr::null_to::<c_char>()),
                Some(value) => {
                    Ok(string::into_leaked_cstring(value.clone())?)
                },
            }
        },
        fail: {
            ptr::null_to::<c_char>()
        }
    }
}

/// Insert the (`key`, `value`) pair into this Message's
/// `metadata` HashMap.
/// This function returns an enum indicating the result;
/// see the comments on HashMapInsertStatus for details.
///
/// # Errors
///
/// This function may fail if the provided `key` or `value` strings
/// contain invalid UTF-8.
#[no_mangle]
#[allow(clippy::missing_safety_doc)]
#[allow(clippy::or_fun_call)]
pub unsafe extern "C" fn message_insert_metadata(
    message: *mut Message,
    key: *const c_char,
    value: *const c_char,
) -> c_int {
    use HashMapInsertStatus as Status;

    ffi! {
        name: "message_insert_metadata",
        params: [message, key, value],
        op: {
            let message = message.as_mut().ok_or(anyhow!("message is null"))?;

            if key.is_null() {
                anyhow::bail!("key is null");
            }

            if value.is_null() {
                anyhow::bail!("value is null");
            }

            let key = CStr::from_ptr(key);
            let key = key
                .to_str()
                .context("error parsing key as UTF-8")?;

            let value = CStr::from_ptr(value);
            let value = value
                .to_str()
                .context("error parsing value as UTF-8")?;

            match message.metadata.insert(key.to_string(), value.to_string()) {
                None => Ok(Status::SuccessNew as c_int),
                Some(_) => Ok(Status::SuccessOverwrite as c_int),
            }
        },
        fail: {
            Status::Error as c_int
        }
    }
}

/*===============================================================================================
 * # Status Types
 *---------------------------------------------------------------------------------------------*/

/// Result from an attempt to insert into a HashMap
enum HashMapInsertStatus {
    /// The value was inserted, and the key was unset
    SuccessNew = 0,
    /// The value was inserted, and the key was previously set
    SuccessOverwrite = -1,
    /// An error occured, and the value was not inserted
    Error = -2,
}
