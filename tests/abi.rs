use super::*;

#[test]
fn abi_layout_and_invalid_arguments_are_explicit() {
    let mut abi = Abi::default();
    assert_eq!(unsafe { orbit_abi(&mut abi) }, 0);
    assert_eq!(abi.version, ABI_VERSION);
    assert_eq!(abi.event_size as usize, size_of::<Event>());
    assert_eq!(abi.options_size as usize, size_of::<OpenOptions>());
    assert_eq!(size_of::<OpenOptions>(), 56);
    assert_eq!(
        unsafe { orbit_abi(ptr::null_mut()) },
        Status::InvalidArgument as u32
    );
    assert_eq!(orbit_cancel(u64::MAX), Status::InvalidHandle as u32);
}

#[test]
fn copy_out_never_overruns_and_reports_required_size() {
    let mut output = [0; 2];
    let mut size = 0;
    assert_eq!(
        unsafe { copy_out(b"abc", output.as_mut_ptr(), 2, &mut size) },
        Err(Status::Capacity)
    );
    assert_eq!(output, [0, 0]);
    assert_eq!(size, 3);
}

#[test]
fn incompatible_abi_is_rejected_before_join() {
    let options = OpenOptions {
        abi_version: u32::MAX,
        incarnation: 1,
        max_in_flight: 1,
        max_queued: 0,
        max_request_bytes: 1,
        max_response_bytes: 1,
        io_buffer_bytes: 1,
    };
    let mut handle = 0;
    assert_eq!(
        unsafe {
            orbit_open(0, ptr::null(), 0, ptr::null(), 0, &options, &mut handle)
        },
        Status::Unsupported as u32
    );
    assert_eq!(handle, 0);
}

#[test]
fn invalid_runtime_options_do_not_create_an_executor() {
    let options = RuntimeOptions {
        abi_version: ABI_VERSION,
        execution: 0,
        native_threads: 0,
    };
    let mut handle = u64::MAX;
    assert_eq!(
        unsafe { orbit_runtime_open(&options, &mut handle) },
        Status::InvalidArgument as u32
    );
    assert_eq!(handle, 0);
    assert_eq!(orbit_runtime_release(u64::MAX), Status::InvalidHandle as u32);
}

#[test]
fn earlier_runtime_record_is_rejected_before_reading_added_fields() {
    #[repr(C)]
    struct EarlierOptions {
        version: u32,
        threads: u32,
    }
    let options = EarlierOptions { version: 1, threads: 1 };
    let mut handle = u64::MAX;
    assert_eq!(
        unsafe {
            orbit_runtime_open(
                (&options as *const EarlierOptions).cast(),
                &mut handle,
            )
        },
        Status::Unsupported as u32
    );
    assert_eq!(handle, 0);
}

#[test]
fn earlier_application_version_is_rejected_before_reading_the_full_record() {
    let version = 3u32;
    let mut handle = u64::MAX;
    assert_eq!(
        unsafe {
            orbit_open(
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                (&version as *const u32).cast(),
                &mut handle,
            )
        },
        Status::Unsupported as u32
    );
    assert_eq!(handle, 0);
}

#[test]
fn execution_modes_validate_thread_counts_without_constructing_a_runtime() {
    use crate::runtime::Execution;
    assert_eq!(Execution::validate(0, 4), Ok(Execution::MultiThread));
    assert_eq!(Execution::validate(1, 1), Ok(Execution::CurrentThread));
    for (execution, threads) in [(0, 0), (1, 0), (1, 2), (2, 1)] {
        assert_eq!(
            Execution::validate(execution, threads),
            Err(Status::InvalidArgument)
        );
    }
}

#[test]
fn recoverable_unwind_becomes_explicit_status() {
    assert_eq!(boundary(|| panic!("ABI test")), Status::Panic as u32);
}

#[test]
fn abi_query_writes_only_its_declared_record_extent() {
    assert_eq!(size_of::<Abi>(), 56);
    #[repr(align(8))]
    struct Storage([u8; 64]);
    let mut storage = Storage([0xa5; 64]);
    assert_eq!(unsafe { orbit_abi(storage.0.as_mut_ptr().cast::<Abi>()) }, 0);
    assert_eq!(&storage.0[56..], &[0xa5; 8]);
}
