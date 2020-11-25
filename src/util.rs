#![allow(dead_code)]

use std::sync::Mutex;
use std::ffi::{VaList, CStr};
use std::os::raw::{c_int, c_char, c_void, c_ulong};
use std::sync::atomic::{self, AtomicBool};
use obs_wrapper::obs_sys::{
    LOG_ERROR, LOG_WARNING, LOG_INFO, LOG_DEBUG,
};
use std::cmp::Ordering;
use std::ops::{Deref, DerefMut};

pub struct Indexed<T> {
    pub index: usize,
    pub inner: T,
}

impl<T> Indexed<T> {
    pub fn into_tuple(self) -> (usize, T) {
        (self.index, self.inner)
    }

    pub fn into_inner(self) -> T {
        self.inner
    }

    pub fn map<R>(self, map: impl FnOnce(T) -> R) -> Indexed<R> {
        Indexed {
            index: self.index,
            inner: (map)(self.inner),
        }
    }
}

impl<T> Indexed<Option<T>> {
    pub fn transpose(self) -> Option<Indexed<T>> {
        let Indexed { index, inner } = self;
        inner.map(|inner| {
            Indexed { index, inner }
        })
    }
}

impl<T, E> Indexed<Result<T, E>> {
    pub fn transpose(self) -> Result<Indexed<T>, E> {
        let Indexed { index, inner } = self;
        inner.map(|inner| {
            Indexed { index, inner }
        })
    }
}

impl<T> From<(usize, T)> for Indexed<T> {
    fn from((index, inner): (usize, T)) -> Self {
        Self { index, inner }
    }
}

impl<T> Deref for Indexed<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for Indexed<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T> PartialEq for Indexed<T> {
    fn eq(&self, other: &Self) -> bool {
        PartialEq::eq(&self.index, &other.index)
    }
}

impl<T> Eq for Indexed<T> {
}

impl<T> PartialOrd for Indexed<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        PartialOrd::partial_cmp(&self.index, &other.index)
    }
}

impl<T> Ord for Indexed<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        Ord::cmp(&self.index, &other.index)
    }
}

#[allow(non_camel_case_types)]
pub type log_handler_t = ::std::option::Option<
    unsafe extern "C" fn(
        lvl: ::std::os::raw::c_int,
        msg: *const ::std::os::raw::c_char,
        args: VaList<'static, 'static>,
        p: *mut ::std::os::raw::c_void,
    ),
>;

extern "C" {
    pub fn base_get_log_handler(
        handler: *mut log_handler_t,
        param: *mut *mut ::std::os::raw::c_void,
    );
}

extern "C" {
    pub fn base_set_log_handler(handler: log_handler_t, param: *mut ::std::os::raw::c_void);
}

extern "C" {
    pub fn vsnprintf<'a>(
        str: *mut ::std::os::raw::c_char,
        size: ::std::os::raw::c_ulong,
        format: *const ::std::os::raw::c_char,
        ap: VaList<'a, 'static>,
    ) -> ::std::os::raw::c_int;
}

pub type RedirectLogCallback = Box<dyn Fn(c_int, *const c_char, VaList<'static, 'static>)>;

#[repr(C)]
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum LogLevel {
    Error = LOG_ERROR as isize,
    Warning = LOG_WARNING as isize,
    Info = LOG_INFO as isize,
    Debug = LOG_DEBUG as isize,
}

static LOG_HANDLER_LOCK: AtomicBool = AtomicBool::new(false);

lazy_static::lazy_static! {
    static ref LOG_CAPTURE_HANDLER: Mutex<Option<LogCaptureHandlerGlobal>> = Mutex::new(None);
}

pub struct LogCaptureHandlerGlobal {
    handler_previous: log_handler_t,
    param_previous: *mut c_void,
    callback_ptr: *mut RedirectLogCallback,
    captured_log: String,
}

unsafe impl Send for LogCaptureHandlerGlobal {}
unsafe impl Sync for LogCaptureHandlerGlobal {}

unsafe extern "C" fn global_redirect_log_handler(
    lvl: c_int,
    msg: *const c_char,
    args: VaList<'static, 'static>,
    p: *mut c_void,
) {
    let callback = Box::from_raw(p as *mut RedirectLogCallback);

    (callback)(lvl, msg, args);

    std::mem::forget(callback);
}

/// Stores its state in LOG_CAPTURE_HANDLER
pub struct LogCaptureHandler;

impl LogCaptureHandler {
    pub fn new(min_log_level: LogLevel) -> Option<Self> {
        if LOG_HANDLER_LOCK.compare_and_swap(false, true, atomic::Ordering::SeqCst) {
            return None;
        }

        let mut handler_previous: log_handler_t = None;
        let mut param_previous = std::ptr::null_mut();
        let handler_previous_ptr: *mut log_handler_t = &mut handler_previous as *mut _;
        let param_previous_ptr: *mut *mut c_void = &mut param_previous as *mut _;

        unsafe {
            base_get_log_handler(handler_previous_ptr, param_previous_ptr);
        }

        let callback_ptr = Box::into_raw(Box::new(Box::new({
            move |log_level, format, args: VaList<'static, 'static>| {
                if let Some(handler_previous) = handler_previous.clone() {
                    unsafe {
                        args.with_copy(move |args| {
                            if log_level <= min_log_level as i32 {
                                const SIZE: usize = 4096;
                                let mut formatted = [0 as c_char; SIZE];
                                let formatted_ptr = &mut formatted[0] as *mut c_char;

                                vsnprintf(formatted_ptr, SIZE as c_ulong, format, args);

                                let formatted = CStr::from_ptr(formatted_ptr);
                                let mut capture_handler = LOG_CAPTURE_HANDLER.lock().unwrap();
                                let capture_handler = capture_handler.as_mut().unwrap();
                                let captured_log = &mut capture_handler.captured_log;

                                *captured_log = format!("{}{}\n", captured_log.clone(), formatted.to_string_lossy());
                            }
                        });

                        // Call the original handler
                        (handler_previous)(log_level, format, args, param_previous);
                    }
                }
            }
        }) as RedirectLogCallback));

        unsafe {
            base_set_log_handler(Some(global_redirect_log_handler), callback_ptr as *mut _);
        }

        *LOG_CAPTURE_HANDLER.lock().unwrap() = Some(LogCaptureHandlerGlobal {
            handler_previous,
            param_previous,
            callback_ptr,
            captured_log: String::new(),
        });

        Some(Self)
    }

    pub fn to_string(self) -> String {
        let captured_log = {
            let capture_handler = LOG_CAPTURE_HANDLER.lock().unwrap();
            let capture_handler = capture_handler.as_ref().unwrap();

            capture_handler.captured_log.clone()
        };

        std::mem::drop(self);

        captured_log
    }
}

impl Drop for LogCaptureHandler {
    fn drop(&mut self) {
        let capture_handler = LOG_CAPTURE_HANDLER.lock().unwrap().take().unwrap();

        unsafe {
            base_set_log_handler(capture_handler.handler_previous, capture_handler.param_previous);

            std::mem::drop(Box::from_raw(capture_handler.callback_ptr as *mut RedirectLogCallback));
        }

        LOG_HANDLER_LOCK.store(false, atomic::Ordering::SeqCst);
    }
}
