pub mod config;
pub mod editor;
pub mod extensions;
pub mod io;
pub mod model;
pub mod ui;

pub fn tune_allocator_for_low_memory() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        unsafe extern "C" {
            fn mallopt(param: i32, value: i32) -> i32;
        }
        const M_TRIM_THRESHOLD: i32 = -1;
        const M_ARENA_MAX: i32 = -8;
        let _ = mallopt(M_ARENA_MAX, 2);
        let _ = mallopt(M_TRIM_THRESHOLD, 128 * 1024);
    }
}

pub fn trim_allocator_memory() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        unsafe extern "C" {
            fn malloc_trim(pad: usize) -> i32;
        }
        let _ = malloc_trim(0);
    }
}
