use std::ffi::{CStr, c_char, c_uint, c_void};
use std::ptr;
use std::sync::Once;
use std::sync::atomic::{AtomicPtr, Ordering};

const DELEGATING_ZONE_NAME: &CStr = c"DelegatingDefaultZoneForPartitionAlloc";
const PARTITION_ALLOC_ZONE_NAME: &CStr = c"PartitionAlloc";
const ZONE_VERSION: c_uint = 13;

type Zone = MallocZone;

#[repr(C)]
struct MallocZone {
    reserved1: *mut c_void,
    reserved2: *mut c_void,
    size: Option<unsafe extern "C" fn(*mut Zone, *const c_void) -> usize>,
    malloc: Option<unsafe extern "C" fn(*mut Zone, usize) -> *mut c_void>,
    calloc: Option<unsafe extern "C" fn(*mut Zone, usize, usize) -> *mut c_void>,
    valloc: Option<unsafe extern "C" fn(*mut Zone, usize) -> *mut c_void>,
    free: Option<unsafe extern "C" fn(*mut Zone, *mut c_void)>,
    realloc: Option<unsafe extern "C" fn(*mut Zone, *mut c_void, usize) -> *mut c_void>,
    destroy: Option<unsafe extern "C" fn(*mut Zone)>,
    zone_name: *const c_char,
    batch_malloc:
        Option<unsafe extern "C" fn(*mut Zone, usize, *mut *mut c_void, c_uint) -> c_uint>,
    batch_free: Option<unsafe extern "C" fn(*mut Zone, *mut *mut c_void, c_uint)>,
    introspect: *mut Introspection,
    version: c_uint,
    memalign: Option<unsafe extern "C" fn(*mut Zone, usize, usize) -> *mut c_void>,
    free_definite_size: Option<unsafe extern "C" fn(*mut Zone, *mut c_void, usize)>,
    pressure_relief: Option<unsafe extern "C" fn(*mut Zone, usize) -> usize>,
    claimed_address: Option<unsafe extern "C" fn(*mut Zone, *mut c_void) -> u32>,
    try_free_default: Option<unsafe extern "C" fn(*mut Zone, *mut c_void)>,
    malloc_with_options: *const c_void,
    malloc_type_malloc: *const c_void,
    malloc_type_calloc: *const c_void,
    malloc_type_realloc: *const c_void,
    malloc_type_memalign: *const c_void,
    malloc_type_malloc_with_options: *const c_void,
}

#[repr(C)]
struct Introspection {
    enumerator: Option<
        unsafe extern "C" fn(u32, *mut c_void, c_uint, usize, *const c_void, *const c_void) -> i32,
    >,
    good_size: Option<unsafe extern "C" fn(*mut Zone, usize) -> usize>,
    check: Option<unsafe extern "C" fn(*mut Zone) -> u32>,
    print: Option<unsafe extern "C" fn(*mut Zone, u32)>,
    log: Option<unsafe extern "C" fn(*mut Zone, *mut c_void)>,
    force_lock: Option<unsafe extern "C" fn(*mut Zone)>,
    force_unlock: Option<unsafe extern "C" fn(*mut Zone)>,
    statistics: Option<unsafe extern "C" fn(*mut Zone, *mut c_void)>,
    zone_locked: Option<unsafe extern "C" fn(*mut Zone) -> u32>,
    enable_discharge_checking: Option<unsafe extern "C" fn(*mut Zone) -> u32>,
    disable_discharge_checking: Option<unsafe extern "C" fn(*mut Zone)>,
    discharge: Option<unsafe extern "C" fn(*mut Zone, *mut c_void)>,
    enumerate_discharged_pointers: *const c_void,
    reinit_lock: Option<unsafe extern "C" fn(*mut Zone)>,
    print_task: *const c_void,
    task_statistics: *const c_void,
    zone_type: c_uint,
}

unsafe extern "C" {
    static mach_task_self_: u32;
    fn malloc_get_all_zones(
        task: u32,
        reader: *const c_void,
        addresses: *mut *mut *mut Zone,
        count: *mut c_uint,
    ) -> i32;
    fn malloc_default_purgeable_zone() -> *mut Zone;
    fn malloc_zone_register(zone: *mut Zone);
    fn malloc_zone_unregister(zone: *mut Zone);
}

static SYSTEM_ZONE: AtomicPtr<Zone> = AtomicPtr::new(ptr::null_mut());

fn system_zone() -> *mut Zone {
    SYSTEM_ZONE.load(Ordering::Acquire)
}

fn zones() -> &'static [*mut Zone] {
    let mut zones = ptr::null_mut();
    let mut count = 0;
    let result = unsafe {
        malloc_get_all_zones(mach_task_self_, ptr::null(), &raw mut zones, &raw mut count)
    };
    if result != 0 || zones.is_null() {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(zones, count as usize) }
}

fn is_registered(name: &CStr) -> bool {
    zones().iter().any(|&zone| {
        let zone_name = unsafe { (*zone).zone_name };
        !zone_name.is_null() && unsafe { CStr::from_ptr(zone_name) } == name
    })
}

unsafe extern "C" fn forward_malloc(_: *mut Zone, size: usize) -> *mut c_void {
    let zone = system_zone();
    unsafe { ((*zone).malloc.unwrap_unchecked())(zone, size) }
}

unsafe extern "C" fn forward_calloc(_: *mut Zone, count: usize, size: usize) -> *mut c_void {
    let zone = system_zone();
    unsafe { ((*zone).calloc.unwrap_unchecked())(zone, count, size) }
}

unsafe extern "C" fn forward_valloc(_: *mut Zone, size: usize) -> *mut c_void {
    let zone = system_zone();
    unsafe { ((*zone).valloc.unwrap_unchecked())(zone, size) }
}

unsafe extern "C" fn forward_realloc(
    _: *mut Zone,
    pointer: *mut c_void,
    size: usize,
) -> *mut c_void {
    let zone = system_zone();
    unsafe { ((*zone).realloc.unwrap_unchecked())(zone, pointer, size) }
}

unsafe extern "C" fn forward_batch_malloc(
    _: *mut Zone,
    size: usize,
    results: *mut *mut c_void,
    count: c_uint,
) -> c_uint {
    let zone = system_zone();
    unsafe { ((*zone).batch_malloc.unwrap_unchecked())(zone, size, results, count) }
}

unsafe extern "C" fn forward_memalign(_: *mut Zone, alignment: usize, size: usize) -> *mut c_void {
    let zone = system_zone();
    unsafe { ((*zone).memalign.unwrap_unchecked())(zone, alignment, size) }
}

unsafe extern "C" fn forward_free(_: *mut Zone, pointer: *mut c_void) {
    let zone = system_zone();
    unsafe { ((*zone).free.unwrap_unchecked())(zone, pointer) }
}

unsafe extern "C" fn forward_free_definite_size(_: *mut Zone, pointer: *mut c_void, size: usize) {
    let zone = system_zone();
    unsafe { ((*zone).free_definite_size.unwrap_unchecked())(zone, pointer, size) }
}

unsafe extern "C" fn forward_batch_free(_: *mut Zone, pointers: *mut *mut c_void, count: c_uint) {
    let zone = system_zone();
    unsafe { ((*zone).batch_free.unwrap_unchecked())(zone, pointers, count) }
}

unsafe extern "C" fn forward_try_free_default(_: *mut Zone, pointer: *mut c_void) {
    let zone = system_zone();
    unsafe { ((*zone).try_free_default.unwrap_unchecked())(zone, pointer) }
}

unsafe extern "C" fn forward_good_size(_: *mut Zone, size: usize) -> usize {
    let zone = system_zone();
    unsafe { ((*(*zone).introspect).good_size.unwrap_unchecked())(zone, size) }
}

unsafe extern "C" fn owns_nothing(_: *mut Zone, _: *const c_void) -> usize {
    0
}

unsafe extern "C" fn relieves_nothing(_: *mut Zone, _: usize) -> usize {
    0
}

unsafe extern "C" fn enumerates_nothing(
    _: u32,
    _: *mut c_void,
    _: c_uint,
    _: usize,
    _: *const c_void,
    _: *const c_void,
) -> i32 {
    0
}

unsafe extern "C" fn passes_check(_: *mut Zone) -> u32 {
    1
}

unsafe extern "C" fn answers_no(_: *mut Zone) -> u32 {
    0
}

unsafe extern "C" fn does_nothing(_: *mut Zone) {}

unsafe extern "C" fn prints_nothing(_: *mut Zone, _: u32) {}

unsafe extern "C" fn ignores_pointer(_: *mut Zone, _: *mut c_void) {}

pub fn install_early_malloc_zone() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(install);
}

fn install() {
    if is_registered(DELEGATING_ZONE_NAME) || is_registered(PARTITION_ALLOC_ZONE_NAME) {
        return;
    }
    let purgeable = unsafe { malloc_default_purgeable_zone() };
    let Some(&system) = zones().first() else {
        return;
    };
    SYSTEM_ZONE.store(system, Ordering::Release);
    let real = unsafe { &*system };
    let has = |present: bool, version: c_uint| present && real.version >= version;

    let introspect = Box::leak(Box::new(Introspection {
        enumerator: Some(enumerates_nothing),
        good_size: Some(forward_good_size),
        check: Some(passes_check),
        print: Some(prints_nothing),
        log: Some(ignores_pointer),
        force_lock: Some(does_nothing),
        force_unlock: Some(does_nothing),
        statistics: Some(ignores_pointer),
        zone_locked: Some(answers_no),
        enable_discharge_checking: Some(answers_no),
        disable_discharge_checking: Some(does_nothing),
        discharge: Some(ignores_pointer),
        enumerate_discharged_pointers: ptr::null(),
        reinit_lock: Some(does_nothing),
        print_task: ptr::null(),
        task_statistics: ptr::null(),
        zone_type: 0,
    }));
    let zone = Box::leak(Box::new(MallocZone {
        reserved1: ptr::null_mut(),
        reserved2: ptr::null_mut(),
        size: Some(owns_nothing),
        malloc: Some(forward_malloc),
        calloc: Some(forward_calloc),
        valloc: Some(forward_valloc),
        free: Some(forward_free),
        realloc: Some(forward_realloc),
        destroy: None,
        zone_name: DELEGATING_ZONE_NAME.as_ptr(),
        batch_malloc: has(real.batch_malloc.is_some(), 0).then_some(forward_batch_malloc as _),
        batch_free: has(real.batch_free.is_some(), 0).then_some(forward_batch_free as _),
        introspect,
        version: ZONE_VERSION,
        memalign: has(real.memalign.is_some(), 5).then_some(forward_memalign as _),
        free_definite_size: has(real.free_definite_size.is_some(), 6)
            .then_some(forward_free_definite_size as _),
        pressure_relief: Some(relieves_nothing),
        claimed_address: None,
        try_free_default: has(real.try_free_default.is_some(), 13)
            .then_some(forward_try_free_default as _),
        malloc_with_options: ptr::null(),
        malloc_type_malloc: ptr::null(),
        malloc_type_calloc: ptr::null(),
        malloc_type_realloc: ptr::null(),
        malloc_type_memalign: ptr::null(),
        malloc_type_malloc_with_options: ptr::null(),
    }));
    let zone: *mut Zone = zone;

    unsafe {
        malloc_zone_register(zone);
        malloc_zone_unregister(system);
        malloc_zone_register(system);
        if !purgeable.is_null() && purgeable != system {
            malloc_zone_unregister(purgeable);
            malloc_zone_register(purgeable);
        }
    }
    debug_assert_eq!(zones().first().copied(), Some(zone));
}
