use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub use app::*;
pub use theme::*;
pub use window::*;

mod app;
mod window;
mod theme;

// use winapi::shared::windef::HWND;
// use winapi::um::libloaderapi::{GetProcAddress, LoadLibraryA};

pub struct Timer {
    inner: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
    is_running: Arc<Mutex<bool>>,
}

impl Timer {
    pub fn new() -> Self {
        Timer {
            inner: Arc::new(Mutex::new(None)),
            is_running: Arc::new(Mutex::new(false)),
        }
    }

    pub fn start<F>(&self, duration: Duration, callback: F)
        where
            F: FnOnce() + Send + 'static,
    {
        let mut is_running = self.is_running.lock().unwrap();
        if *is_running {
            panic!("Timer is already running!");
        }
        *is_running = true;
        drop(is_running);

        let inner = Arc::clone(&self.inner);
        let is_running = Arc::clone(&self.is_running);
        let handle = thread::spawn(move || {
            thread::park_timeout(duration);
            let mut is_running = is_running.lock().unwrap();
            if *is_running {
                *is_running = false;
                drop(is_running);
                callback();
            }
        });

        let mut guard = inner.lock().unwrap();
        *guard = Some(handle);
    }

    pub fn cancel(&self) {
        let mut guard = self.inner.lock().unwrap();
        if let Some(handle) = guard.take() {
            let mut is_running = self.is_running.lock().unwrap();
            *is_running = false;
            drop(guard);
            handle.thread().unpark();
        }
    }
}
/*
#[repr(C)]
pub enum WindowCompositionAttrib {
    Undefined = 0,
    NcRenderingEnabled = 1,
    NcRenderingPolicy = 2,
    TransitionsForceDisabled = 3,
    AllowNCPaint = 4,
    CaptionButtonBounds = 5,
    NonClientRTLLayout = 6,
    ForceIconicRepresentation = 7,
    ExtendedFrameBounds = 8,
    HasIconicBitmap = 9,
    ThemeAttributes = 10,
    NcRenderingExiled = 11,
    NcAdornmentInfo = 12,
    ExcludedFromLivePreview = 13,
    VideoOverlayActive = 14,
    ForceActiveWindowAppearance = 15,
    DisallowPeek = 16,
    Cloak = 17,
    Cloaked = 18,
    AccentPolicy = 19,
    FreezeRepresentation = 20,
    EverUncloaked = 21,
    VisualOwner = 22,
    Holographic = 23,
    ExcludedFromDDA = 24,
    PassiveUpdateMode = 25,
    Last = 26,
}

#[repr(C)]
pub enum AccentState {
    Disabled = 0,
    EnableGradient = 1,
    EnableTransparentGradient = 2,
    EnableBlurBehind = 3,
    EnableAcrylicBlurBehind = 4,
    EnableHostBackdrop = 5,
    InvalidState = 6,
}

#[repr(C)]
pub struct AccentPolicy {
    pub accent_state: AccentState,
    pub accent_flags: u32,
    pub gradient_color: u32,
    pub animation_id: u32,
}

#[repr(C)]
pub struct WindowCompositionAttribData {
    pub attribute: WindowCompositionAttrib,
    pub data: *mut c_void,
    pub size_of_data: u32,
}


type SetWindowCompositionAttribute = unsafe extern "system" fn(
    hwnd: *mut c_void,
    data: *mut WindowCompositionAttribData,
) -> I32;

pub fn set_acrylic(hwnd: *mut c_void){
    let lib = unsafe { LoadLibraryA("user32.dll\0".as_ptr() as *const i8) };
    let func = unsafe { GetProcAddress(lib, "SetWindowCompositionAttribute\0".as_ptr() as *const i8) };
    let func = unsafe { std::mem::transmute::<_, SetWindowCompositionAttribute>(func) };
    let accent = AccentPolicy {
        accent_state: AccentState::EnableAcrylicBlurBehind,
        accent_flags: 0,
        gradient_color: 0x00FFFFFF,
        animation_id: 0,
    };
    let mut data = WindowCompositionAttribData {
        attribute: WindowCompositionAttrib::AccentPolicy,
        data: &accent as *const _ as *mut _,
        size_of_data: std::mem::size_of::<AccentPolicy>() as u32,
    };
    unsafe {
        func(hwnd, &mut data as *mut _);
    }
}

pub fn set_areo(hwnd: *mut c_void){
    let lib = unsafe { LoadLibraryA("user32.dll\0".as_ptr() as *const i8) };
    let func = unsafe { GetProcAddress(lib, "SetWindowCompositionAttribute\0".as_ptr() as *const i8) };
    let func = unsafe { std::mem::transmute::<_, SetWindowCompositionAttribute>(func) };
    let accent = AccentPolicy {
        accent_state: AccentState::EnableBlurBehind,
        accent_flags: 0,
        gradient_color: 0x00FFFFFF,
        animation_id: 0,
    };
    let mut data = WindowCompositionAttribData {
        attribute: WindowCompositionAttrib::AccentPolicy,
        data: &accent as *const _ as *mut _,
        size_of_data: std::mem::size_of::<AccentPolicy>() as u32,
    };
    unsafe {
        func(hwnd, &mut data as *mut _);
    }
}

pub fn set_transparent(hwnd: *mut c_void){
    let lib = unsafe { LoadLibraryA("user32.dll\0".as_ptr() as *const i8) };
    let func = unsafe { GetProcAddress(lib, "SetWindowCompositionAttribute\0".as_ptr() as *const i8) };
    let func = unsafe { std::mem::transmute::<_, SetWindowCompositionAttribute>(func) };
    let accent = AccentPolicy {
        accent_state: AccentState::EnableTransparentGradient,
        accent_flags: 0,
        gradient_color: 0x00FFFFFF,
        animation_id: 0,
    };
    let mut data = WindowCompositionAttribData {
        attribute: WindowCompositionAttrib::AccentPolicy,
        data: &accent as *const _ as *mut _,
        size_of_data: std::mem::size_of::<AccentPolicy>() as u32,
    };
    unsafe {
        func(hwnd, &mut data as *mut _);
    }
}*/