//! Windows CNG HMAC and private native-call seam for failure/resource tests.
use crate::Result;
use std::{ffi::c_void, ptr};

type Handle = *mut c_void;
type Open = unsafe extern "system" fn(*mut Handle, *const u16, *const u16, u32) -> i32;
type Hash = unsafe extern "system" fn(Handle, *const u8, u32, *const u8, u32, *mut u8, u32) -> i32;
type Close = unsafe extern "system" fn(Handle, u32) -> i32;

#[link(name = "bcrypt")]
unsafe extern "system" {
    fn BCryptOpenAlgorithmProvider(
        handle: *mut Handle,
        alg: *const u16,
        implementation: *const u16,
        flags: u32,
    ) -> i32;
    fn BCryptHash(
        handle: Handle,
        secret: *const u8,
        secret_len: u32,
        input: *const u8,
        input_len: u32,
        output: *mut u8,
        output_len: u32,
    ) -> i32;
    fn BCryptCloseAlgorithmProvider(handle: Handle, flags: u32) -> i32;
}

#[derive(Clone, Copy)]
struct Api {
    open: Open,
    hash: Hash,
    close: Close,
}

const NATIVE: Api = Api {
    open: BCryptOpenAlgorithmProvider,
    hash: BCryptHash,
    close: BCryptCloseAlgorithmProvider,
};

pub(super) fn hmac(secret: &[u8], bytes: &[u8]) -> Result<[u8; 32]> {
    hmac_with_api(secret, bytes, NATIVE)
}

fn hmac_with_api(secret: &[u8], bytes: &[u8], api: Api) -> Result<[u8; 32]> {
    let key_len = u32::try_from(secret.len()).map_err(|_| "SECRET_SIZE")?;
    let input_len = u32::try_from(bytes.len()).map_err(|_| "PAYLOAD_SIZE")?;
    let algorithm: Vec<u16> = "SHA256\0".encode_utf16().collect();
    let mut handle = ptr::null_mut();
    let mut output = [0; 32];
    // SAFETY: only the private native API (or a controlled unit-test seam)
    // receives live bounded buffers. No fallible Rust work occurs between a
    // successful open and the single close attempt. HMAC flag is 0x8.
    unsafe {
        if (api.open)(&mut handle, algorithm.as_ptr(), ptr::null(), 8) < 0 {
            return Err("CNG_OPEN_FAILED");
        }
        let status = (api.hash)(
            handle,
            secret.as_ptr(),
            key_len,
            bytes.as_ptr(),
            input_len,
            output.as_mut_ptr(),
            32,
        );
        let closed = (api.close)(handle, 0);
        // A failed close leaves resource release unconfirmed. Preserve that
        // outcome separately, including when hashing failed too; never retry
        // the close blindly or report a digest after an unconfirmed cleanup.
        match (status < 0, closed < 0) {
            (true, true) => return Err("CNG_HASH_AND_CLOSE_FAILED"),
            (true, false) => return Err("CNG_HASH_FAILED"),
            (false, true) => return Err("CNG_CLOSE_FAILED"),
            (false, false) => {}
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse_signature, verify_signature};
    use std::cell::RefCell;

    #[derive(Default, Debug)]
    struct Trace {
        open_status: i32,
        hash_status: i32,
        close_status: i32,
        open_calls: usize,
        hash_calls: usize,
        close_calls: usize,
        successful_opens: usize,
        successful_closes: usize,
        live_providers: usize,
        bad_parameters: bool,
    }

    thread_local! { static TRACE: RefCell<Trace> = RefCell::new(Trace::default()); }

    fn reset(trace: Trace) {
        TRACE.with(|state| *state.borrow_mut() = trace);
    }

    // These fake callbacks never cross into CNG and never dereference the
    // sentinel handle. Their tests cover wrapper classification/call order only.
    unsafe extern "system" fn injected_open(
        handle: *mut Handle,
        algorithm: *const u16,
        implementation: *const u16,
        flags: u32,
    ) -> i32 {
        TRACE.with(|state| {
            let mut trace = state.borrow_mut();
            trace.open_calls += 1;
            if handle.is_null() || algorithm.is_null() || !implementation.is_null() || flags != 8 {
                trace.bad_parameters = true;
                return -1;
            }
            if trace.open_status >= 0 {
                // SAFETY: hmac_with_api passes its live handle output slot.
                unsafe { handle.write(ptr::dangling_mut::<u8>().cast()) };
                trace.successful_opens += 1;
                trace.live_providers += 1;
            }
            trace.open_status
        })
    }

    unsafe extern "system" fn injected_hash(
        handle: Handle,
        _secret: *const u8,
        _secret_len: u32,
        _input: *const u8,
        _input_len: u32,
        output: *mut u8,
        output_len: u32,
    ) -> i32 {
        TRACE.with(|state| {
            let mut trace = state.borrow_mut();
            trace.hash_calls += 1;
            if handle != ptr::dangling_mut::<u8>().cast() || output.is_null() || output_len != 32 {
                trace.bad_parameters = true;
                return -1;
            }
            trace.hash_status
        })
    }

    unsafe extern "system" fn injected_close(handle: Handle, flags: u32) -> i32 {
        TRACE.with(|state| {
            let mut trace = state.borrow_mut();
            trace.close_calls += 1;
            if handle != ptr::dangling_mut::<u8>().cast() || flags != 0 || trace.live_providers != 1
            {
                trace.bad_parameters = true;
                return -1;
            }
            if trace.close_status >= 0 {
                trace.successful_closes += 1;
                trace.live_providers -= 1;
            }
            trace.close_status
        })
    }

    const INJECTED: Api = Api {
        open: injected_open,
        hash: injected_hash,
        close: injected_close,
    };

    #[test]
    fn cng_injected_close_failure_is_distinct_from_hash_failure() {
        reset(Trace {
            close_status: -1,
            ..Trace::default()
        });
        assert_eq!(
            hmac_with_api(b"public-test-key-16", b"public data", INJECTED),
            Err("CNG_CLOSE_FAILED")
        );
        TRACE.with(|state| {
            let trace = state.borrow();
            assert_eq!(
                (trace.open_calls, trace.hash_calls, trace.close_calls),
                (1, 1, 1)
            );
            assert_eq!(trace.live_providers, 1); // Injected cleanup remains uncertain.
            assert!(!trace.bad_parameters);
        });
    }

    #[test]
    fn cng_injected_hash_and_close_failures_preserve_both_outcomes() {
        reset(Trace {
            hash_status: -1,
            close_status: -1,
            ..Trace::default()
        });
        assert_eq!(
            hmac_with_api(b"public-test-key-16", b"public data", INJECTED),
            Err("CNG_HASH_AND_CLOSE_FAILED")
        );
        TRACE.with(|state| {
            let trace = state.borrow();
            assert_eq!(
                (trace.open_calls, trace.hash_calls, trace.close_calls),
                (1, 1, 1)
            );
            assert_eq!(trace.live_providers, 1);
            assert!(!trace.bad_parameters);
        });
    }

    #[test]
    fn cng_injected_open_failure_never_hashes_or_closes() {
        reset(Trace {
            open_status: -1,
            ..Trace::default()
        });
        assert_eq!(
            hmac_with_api(b"public-test-key-16", b"public data", INJECTED),
            Err("CNG_OPEN_FAILED")
        );
        TRACE.with(|state| {
            let trace = state.borrow();
            assert_eq!(
                (trace.open_calls, trace.hash_calls, trace.close_calls),
                (1, 0, 0)
            );
            assert_eq!(trace.live_providers, 0);
            assert!(!trace.bad_parameters);
        });
    }

    #[test]
    fn cng_injected_hash_failure_attempts_close_exactly_once() {
        reset(Trace {
            hash_status: -1,
            ..Trace::default()
        });
        assert_eq!(
            hmac_with_api(b"public-test-key-16", b"public data", INJECTED),
            Err("CNG_HASH_FAILED")
        );
        TRACE.with(|state| {
            let trace = state.borrow();
            assert_eq!(
                (trace.open_calls, trace.hash_calls, trace.close_calls),
                (1, 1, 1)
            );
            assert_eq!(trace.live_providers, 0);
            assert!(!trace.bad_parameters);
        });
    }

    // These callbacks call the real Windows functions. Only parameters needed
    // to obtain a documented native failure are changed by fault-injection tests.
    unsafe extern "system" fn native_open(
        handle: *mut Handle,
        algorithm: *const u16,
        implementation: *const u16,
        flags: u32,
    ) -> i32 {
        // SAFETY: forwarded live output slot and NUL-terminated algorithm from wrapper.
        let status =
            unsafe { BCryptOpenAlgorithmProvider(handle, algorithm, implementation, flags) };
        TRACE.with(|state| {
            let mut trace = state.borrow_mut();
            trace.open_calls += 1;
            trace.open_status = status;
            if status >= 0 {
                trace.successful_opens += 1;
                trace.live_providers += 1;
            }
        });
        status
    }

    unsafe extern "system" fn native_bad_algorithm(
        handle: *mut Handle,
        _algorithm: *const u16,
        implementation: *const u16,
        flags: u32,
    ) -> i32 {
        const UNKNOWN: [u16; 16] = [
            67, 87, 71, 95, 78, 79, 95, 83, 85, 67, 72, 95, 65, 76, 71, 0,
        ];
        // SAFETY: local fixed NUL-terminated identifier, same valid output slot.
        unsafe { native_open(handle, UNKNOWN.as_ptr(), implementation, flags) }
    }

    unsafe extern "system" fn native_hash(
        handle: Handle,
        secret: *const u8,
        secret_len: u32,
        input: *const u8,
        input_len: u32,
        output: *mut u8,
        output_len: u32,
    ) -> i32 {
        // SAFETY: provider and exact live buffer bounds come from hmac_with_api.
        let status = unsafe {
            BCryptHash(
                handle, secret, secret_len, input, input_len, output, output_len,
            )
        };
        TRACE.with(|state| {
            let mut trace = state.borrow_mut();
            trace.hash_calls += 1;
            trace.hash_status = status;
        });
        status
    }

    unsafe extern "system" fn native_bad_output_length(
        handle: Handle,
        secret: *const u8,
        secret_len: u32,
        input: *const u8,
        input_len: u32,
        output: *mut u8,
        output_len: u32,
    ) -> i32 {
        // SHA256 requires exactly 32 output bytes. The underlying buffer still
        // has 32 live bytes; advertising 31 safely forces a real CNG error.
        let wrong_length = output_len.saturating_sub(1);
        // SAFETY: supplied output capacity is reduced, never enlarged.
        unsafe {
            native_hash(
                handle,
                secret,
                secret_len,
                input,
                input_len,
                output,
                wrong_length,
            )
        }
    }

    unsafe extern "system" fn native_close(handle: Handle, flags: u32) -> i32 {
        // SAFETY: close the real provider created by the matching native open.
        let status = unsafe { BCryptCloseAlgorithmProvider(handle, flags) };
        TRACE.with(|state| {
            let mut trace = state.borrow_mut();
            trace.close_calls += 1;
            trace.close_status = status;
            if status >= 0 {
                trace.successful_closes += 1;
                match trace.live_providers.checked_sub(1) {
                    Some(count) => trace.live_providers = count,
                    None => trace.bad_parameters = true,
                }
            }
        });
        status
    }

    const TRACED_NATIVE: Api = Api {
        open: native_open,
        hash: native_hash,
        close: native_close,
    };

    #[test]
    fn cng_native_open_failure_creates_no_owned_provider() {
        reset(Trace::default());
        let api = Api {
            open: native_bad_algorithm,
            ..TRACED_NATIVE
        };
        assert_eq!(
            hmac_with_api(b"public-test-key-16", b"public data", api),
            Err("CNG_OPEN_FAILED")
        );
        TRACE.with(|state| {
            let trace = state.borrow();
            println!("real CNG invalid algorithm: {trace:?}");
            assert!(trace.open_status < 0);
            assert_eq!(
                (trace.open_calls, trace.hash_calls, trace.close_calls),
                (1, 0, 0)
            );
            assert_eq!(trace.live_providers, 0);
        });
    }

    #[test]
    fn cng_native_hash_failure_closes_real_provider_once() {
        reset(Trace::default());
        let api = Api {
            hash: native_bad_output_length,
            ..TRACED_NATIVE
        };
        assert_eq!(
            hmac_with_api(b"public-test-key-16", b"public data", api),
            Err("CNG_HASH_FAILED")
        );
        TRACE.with(|state| {
            let trace = state.borrow();
            println!("real CNG invalid output size: {trace:?}");
            assert_eq!(trace.open_status, 0);
            assert!(trace.hash_status < 0);
            assert_eq!(trace.close_status, 0);
            assert_eq!(
                (
                    trace.successful_opens,
                    trace.successful_closes,
                    trace.live_providers
                ),
                (1, 1, 0)
            );
            assert_eq!(
                (trace.open_calls, trace.hash_calls, trace.close_calls),
                (1, 1, 1)
            );
            assert!(!trace.bad_parameters);
        });
    }

    #[test]
    fn cng_native_repeated_real_providers_close_with_balanced_successes() {
        reset(Trace::default());
        let expected = parse_signature(
            "sha256=773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe",
        )
        .unwrap();
        for _ in 0..256 {
            assert_eq!(
                hmac_with_api(&[0xaa; 20], &[0xdd; 50], TRACED_NATIVE).unwrap(),
                expected
            );
        }
        TRACE.with(|state| {
            let trace = state.borrow();
            println!("real CNG provider lifetime count: {trace:?}");
            assert_eq!(
                (trace.open_calls, trace.hash_calls, trace.close_calls),
                (256, 256, 256)
            );
            assert_eq!(
                (
                    trace.successful_opens,
                    trace.successful_closes,
                    trace.live_providers
                ),
                (256, 256, 0)
            );
            assert!(!trace.bad_parameters);
        });
    }

    #[test]
    fn cng_native_rfc4231_vectors_and_exact_binary_bytes() {
        // RFC 4231 cases 1, 3, 6: https://www.rfc-editor.org/rfc/rfc4231.txt
        let case1 = "sha256=b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7";
        assert_eq!(verify_signature(&[0x0b; 20], b"Hi There", case1), Ok(()));
        for altered in [
            b"Hi There\0".as_slice(),
            b"Hi There\r\n",
            b"Hi There\n",
            b"Hi There\xff",
            b"",
        ] {
            assert_eq!(
                verify_signature(&[0x0b; 20], altered, case1),
                Err("SIGNATURE_MISMATCH")
            );
        }
        let case3 = "sha256=773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe";
        assert_eq!(verify_signature(&[0xaa; 20], &[0xdd; 50], case3), Ok(()));
        let case6 = "sha256=60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54";
        assert_eq!(
            verify_signature(
                &[0xaa; 131],
                b"Test Using Larger Than Block-Size Key - Hash Key First",
                case6
            ),
            Ok(())
        );
    }

    #[test]
    fn cng_native_empty_payload_and_secret_size_policy_boundaries() {
        // Public all-zero test keys. Expected empty-input MACs were generated
        // independently with the existing Node/OpenSSL HMAC API, not with CNG.
        for (size, header) in [
            (
                16,
                "sha256=b613679a0814d9ec772f95d778c35fc5ff1697c493715653c6c712144292c5ad",
            ),
            (
                1024,
                "sha256=69adcb6605f38cf747c40fd802ca80e7970abbb81980ea5902f83c66d6e9d6c0",
            ),
        ] {
            assert_eq!(verify_signature(&vec![0; size], b"", header), Ok(()));
        }
        let header = "sha256=b613679a0814d9ec772f95d778c35fc5ff1697c493715653c6c712144292c5ad";
        for size in [0, 15, 1025] {
            assert_eq!(
                verify_signature(&vec![0; size], b"", header),
                Err("SECRET_SIZE")
            );
        }
    }
}
