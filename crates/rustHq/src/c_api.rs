#![allow(clippy::not_unsafe_ptr_arg_deref)]

use crate::manager::TdxDataManager;
use crate::models::{KlineType, TaskData};
use std::ffi::c_char;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

#[repr(C)]
pub struct TdxKlineDataC {
    pub datetime: [c_char; 32],
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub amount: f64,
}

#[repr(C)]
pub struct TdxTaskResultC {
    pub stock_code: [c_char; 16],
    pub success: i32,
    pub data_count: i32,
    pub data: *mut TdxKlineDataC,
    pub error_message: [c_char; 256],
    pub download_time: i32,
    pub server_used: [c_char; 64],
}

fn error_buffer() -> &'static Mutex<Vec<u8>> {
    static ERROR_BUFFER: OnceLock<Mutex<Vec<u8>>> = OnceLock::new();
    ERROR_BUFFER.get_or_init(|| Mutex::new(b"ok\0".to_vec()))
}

fn set_last_error(message: impl AsRef<str>) {
    let message = message
        .as_ref()
        .bytes()
        .filter(|value| *value != 0)
        .collect::<Vec<_>>();
    let mut buffer = error_buffer().lock().unwrap();
    *buffer = message;
    buffer.push(0);
}

unsafe fn ptr_to_manager<'a>(manager: *mut TdxDataManager) -> Option<&'a mut TdxDataManager> {
    if manager.is_null() {
        set_last_error("null manager");
        None
    } else {
        Some(unsafe { &mut *manager })
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn createTdxDataManager() -> *mut TdxDataManager {
    Box::into_raw(Box::new(TdxDataManager::new()))
}

#[unsafe(no_mangle)]
pub extern "C" fn destroyTdxDataManager(manager: *mut TdxDataManager) {
    if manager.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(manager));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn initializeTdxDataManager(
    manager: *mut TdxDataManager,
    servers: *const *const c_char,
    server_count: i32,
    _max_threads_per_server: i32,
) -> i32 {
    let Some(manager) = (unsafe { ptr_to_manager(manager) }) else {
        return 0;
    };

    let server_list = read_c_string_list(servers, server_count);
    if manager.initialize(&server_list, &[], server_list.len().max(1)) {
        1
    } else {
        set_last_error("failed to initialize manager");
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn scheduleTasks(
    manager: *mut TdxDataManager,
    stock_codes: *const *const c_char,
    stock_counts: *const i32,
    stock_count: i32,
    kline_type: i32,
) -> i32 {
    let Some(manager) = (unsafe { ptr_to_manager(manager) }) else {
        return 0;
    };
    if stock_codes.is_null() || stock_counts.is_null() || stock_count <= 0 {
        set_last_error("invalid task inputs");
        return 0;
    }

    let mut tasks = std::collections::BTreeMap::new();
    for index in 0..stock_count as isize {
        let code_ptr = unsafe { *stock_codes.offset(index) };
        let code = read_c_string(code_ptr);
        let count = unsafe { *stock_counts.offset(index) };
        tasks.insert(code, count.max(0) as usize);
    }

    if manager.schedule_tasks(&tasks, KlineType::from_i32(kline_type)) {
        1
    } else {
        set_last_error("failed to schedule tasks");
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn waitForAllTasks(manager: *mut TdxDataManager, timeout_ms: i32) -> i32 {
    let Some(manager) = (unsafe { ptr_to_manager(manager) }) else {
        return 0;
    };
    if manager.wait_for_all_tasks(Duration::from_millis(timeout_ms.max(0) as u64)) {
        1
    } else {
        set_last_error("wait timed out");
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn getCompletedResults(
    manager: *mut TdxDataManager,
    results: *mut TdxTaskResultC,
    max_results: i32,
) -> i32 {
    let Some(manager) = (unsafe { ptr_to_manager(manager) }) else {
        return 0;
    };
    if results.is_null() || max_results <= 0 {
        set_last_error("invalid result buffer");
        return 0;
    }

    let items = manager.get_completed_results_limit(max_results as usize);
    for (index, item) in items.iter().enumerate() {
        let slot = unsafe { &mut *results.add(index) };
        *slot = zero_result_slot();
        write_fixed(&mut slot.stock_code, &item.stock_code);
        write_fixed(&mut slot.error_message, &item.error_message);
        write_fixed(&mut slot.server_used, &item.server_used);
        slot.success = i32::from(item.success);
        slot.download_time = item.download_time.as_millis().min(i32::MAX as u128) as i32;

        if let TaskData::Klines(values) = &item.data {
            let mut c_rows = values
                .iter()
                .map(|row| {
                    let mut c_row = TdxKlineDataC {
                        datetime: [0; 32],
                        open: row.open,
                        high: row.high,
                        low: row.low,
                        close: row.close,
                        volume: row.volume,
                        amount: row.amount,
                    };
                    write_fixed(&mut c_row.datetime, &row.datetime);
                    c_row
                })
                .collect::<Vec<_>>();
            slot.data_count = c_rows.len() as i32;
            slot.data = c_rows.as_mut_ptr();
            std::mem::forget(c_rows);
        }
    }

    items.len() as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn getStatistics(
    manager: *mut TdxDataManager,
    total_tasks: *mut i32,
    completed_tasks: *mut i32,
    connected_threads: *mut i32,
) -> i32 {
    let Some(manager) = (unsafe { ptr_to_manager(manager) }) else {
        return 0;
    };
    let stats = manager.get_statistics();
    unsafe {
        if !total_tasks.is_null() {
            *total_tasks = stats.total_tasks.min(i32::MAX as usize) as i32;
        }
        if !completed_tasks.is_null() {
            *completed_tasks = stats.completed_tasks.min(i32::MAX as usize) as i32;
        }
        if !connected_threads.is_null() {
            *connected_threads = stats.connected_threads.min(i32::MAX as usize) as i32;
        }
    }
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn freeResults(results: *mut TdxTaskResultC, count: i32) {
    if results.is_null() || count <= 0 {
        return;
    }
    for index in 0..count as usize {
        let slot = unsafe { &mut *results.add(index) };
        if !slot.data.is_null() && slot.data_count > 0 {
            unsafe {
                drop(Vec::from_raw_parts(
                    slot.data,
                    slot.data_count as usize,
                    slot.data_count as usize,
                ));
            }
            slot.data = std::ptr::null_mut();
            slot.data_count = 0;
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn getVersion() -> *const c_char {
    static VERSION: &[u8] = b"dllhqarrow-rs/0.1.0\0";
    VERSION.as_ptr() as *const c_char
}

#[unsafe(no_mangle)]
pub extern "C" fn getLastError() -> *const c_char {
    error_buffer().lock().unwrap().as_ptr() as *const c_char
}

fn read_c_string_list(ptr: *const *const c_char, count: i32) -> Vec<String> {
    if ptr.is_null() || count <= 0 {
        return Vec::new();
    }
    (0..count as isize)
        .map(|index| unsafe { read_c_string(*ptr.offset(index)) })
        .collect()
}

fn read_c_string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut bytes = Vec::new();
    let mut offset = 0;
    loop {
        let value = unsafe { *ptr.add(offset) };
        if value == 0 {
            break;
        }
        bytes.push(value as u8);
        offset += 1;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn write_fixed<const N: usize>(buffer: &mut [c_char; N], value: &str) {
    buffer.fill(0);
    let bytes = value.as_bytes();
    let len = bytes.len().min(N.saturating_sub(1));
    for (index, byte) in bytes.iter().take(len).enumerate() {
        buffer[index] = *byte as c_char;
    }
}

fn zero_result_slot() -> TdxTaskResultC {
    TdxTaskResultC {
        stock_code: [0; 16],
        success: 0,
        data_count: 0,
        data: std::ptr::null_mut(),
        error_message: [0; 256],
        server_used: [0; 64],
        download_time: 0,
    }
}
