#![feature(let_chains)]

//! ```
//! use tkym_windows::Platform;
//! let platform = Platform::init();
//! let window = Platform.create_window();
//! loop {
//!     window.process_messages();
//!     // ...
//! }
//! ```
use std::cell::RefCell;
use std::{collections::HashMap};
use std::pin::Pin;
use windows::{core::*, s, Win32::{Foundation::*, Graphics::Gdi::*, System::LibraryLoader::*, UI::WindowsAndMessaging::*, }, };

use common::geo::{Vector2, Rect2};

const CLASS_NAME: PCSTR = s!("RGUIWC");

fn instance_handle() -> HINSTANCE {
    unsafe { GetModuleHandleA(None).unwrap() }
}

#[derive(Debug, Copy, Clone)]
pub struct Window {
    handle:         HWND,
    device_context: HDC,
    pub minimized: bool,
    /// Details of the mouse, as last captured by the window.
    pub mouse: Mouse,
    /// The current size of the window.
    pub size:  Rect2,
    
}

impl Window {
    pub fn new(handle: HWND) -> Self {
        Self { 
            handle, 
            minimized: false, 
            mouse: Mouse::default(), 
            size:  Rect2::new(900, 600), 
            device_context: unsafe { GetDC(handle) }
        }
    }

    pub fn process_messages(&self) {
        let mut msg = MSG::default();
        unsafe {
            while PeekMessageA(&mut msg, self.handle, 0, 0, PM_REMOVE) != BOOL(0) {
                TranslateMessage(&mut msg);
                DispatchMessageA(&mut msg);
            }
        }
    }

    pub fn swap_buffers<T: Into<*const u8>>(&self, buffer: T) {
        let Rect2 { width, height } = self.size;
        unsafe {
            StretchDIBits(
                self.device_context,
                0,
                0,
                width  as i32,
                height as i32,
                0,
                0,
                width  as i32,
                height as i32,
                Some(buffer.into().cast()),
                &BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize:        std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth:       width as i32,
                        biHeight:      -(height as i32),
                        biPlanes:      1,
                        biBitCount:    32,
                        biCompression: BI_RGB,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                DIB_RGB_COLORS,
                SRCCOPY,
            );
        }
    }
}

/// State for windows, to be managed by the window procedure.
#[derive(Debug, Copy, Clone)]
pub struct State {
    
}

#[derive(Debug, Copy, Clone)]
pub struct Mouse {
    /// The current mouse position in window coordinates.
    pub pos:   Vector2,
    // Buttons
    pub left:  bool,
    pub right: bool,
}

impl Default for Mouse {
    fn default() -> Self {
        Self {
            pos:   Vector2::new(0,0),
            left:  false,
            right: false,
        }
    }
}

#[derive(Debug)]
pub struct Platform {
    pub handle: HINSTANCE,
    pub windows: Pin<Box<RefCell<WindowMap>>>
}

impl Platform {
    pub fn init() -> Self {
        let handle = instance_handle();
        unsafe {
            RegisterClassA(&WNDCLASSA {
                style:         CS_HREDRAW | CS_HREDRAW | CS_OWNDC, 
                hInstance:     handle,
                hCursor:       HCURSOR(0),
                hIcon:         HICON(0),
                lpszClassName: CLASS_NAME,
                lpfnWndProc:   Some(window_proc),
                ..Default::default()
            });
        };
        let windows = Box::pin(RefCell::new(HashMap::new()));
        Self { handle, windows }
    }

    pub fn create_window(&self) -> WindowHandle {
        let window_name = s!("Rust GUI");
        let win_handle = unsafe {
            CreateWindowExA(
                WINDOW_EX_STYLE(0),
                CLASS_NAME,
                window_name,
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                900_i32,
                600_i32,
                HWND(0),
                HMENU(0),
                self.handle,
                Some((self.windows.as_ref().get_ref() as *const RefCell<WindowMap>).cast()),
            )
        };
        let window = Window::new(win_handle);

        (*self.windows).borrow_mut().insert(window.handle.0, window);
        WindowHandle(win_handle, &self)
    }
}

/// Interface for consumer to interact with created windows.
#[derive(Debug)]
pub struct WindowHandle<'p>(HWND, &'p Platform);

impl<'p> WindowHandle<'p> {
    pub fn state(&self) -> Window {
        *(self.1).windows.borrow().get(&self.0.0).unwrap()
    }
}

type WindowMap = HashMap<isize, Window>;

#[derive(Clone, Copy)]
pub(crate) struct ProcArgs {
    win_handle: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
}

mod subprocedures {
    use super::*;
    pub(crate) fn wm_create(args: ProcArgs) {
        let ProcArgs { win_handle, lparam, .. } = args;
        if let Some(state) = unsafe { (lparam.0 as *const CREATESTRUCTA).as_ref() } {
            println!("{state:?}");
            let list_ptr = state.lpCreateParams as isize as *const RefCell<WindowMap>;
            if let Some(state) = unsafe { list_ptr.as_ref() } && let Ok(mut window_list) = state.try_borrow_mut() {
                (*window_list).insert(win_handle.0, Window::new(win_handle));
            }
            unsafe {
                SetPropA(win_handle, s!("window_list"), windows::Win32::Foundation::HANDLE(state.lpCreateParams as isize));
            }
            // SetWindowPos(win_handle, None, 0 ,0 ,0 , 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOMOVE | SWP_FRAMECHANGED);
        }
    }

    pub(crate) fn wm_mouse_move(window: &mut Window, args: ProcArgs) {
        let ProcArgs { win_handle, lparam, .. } = args;
        use std::mem::transmute;
        // optimize: would reading off a raw pointer be faster than byte splitting?
        let lparam_bytes = lparam.0.to_le_bytes();
        let (x,y) = unsafe {
            (
                transmute::<[u8; 2], u16>([lparam_bytes[0], lparam_bytes[1]]),
                transmute::<[u8; 2], u16>([lparam_bytes[2], lparam_bytes[3]])
            )
        };
        window.mouse.pos = Vector2::new(x as u32, y as u32);
    }

    pub(crate) fn wm_size(window: &mut Window, args: ProcArgs) {
        let ProcArgs { win_handle, .. } = args;
        let mut rect = RECT::default();
        unsafe {
            GetClientRect(win_handle, &mut rect);
        }
        let width  = rect.right  as u32;
        let height = rect.bottom as u32;
        window.size = Rect2::new(width,height);
    }

    pub(crate) fn wm_l_button_down(window: &mut Window, args: ProcArgs) {
        window.mouse.left = true;
    }

    pub(crate) fn wm_l_button_up(window: &mut Window, args: ProcArgs) {
        window.mouse.left = false;
    }

    pub(crate) fn wm_r_button_down(window: &mut Window, args: ProcArgs) {
        window.mouse.right = true;
    }

    pub(crate) fn wm_r_button_up(window: &mut Window, args: ProcArgs) {
        window.mouse.right = false;
    }
}


unsafe extern "system" fn window_proc(
    win_handle: HWND,
    message:    u32,
    wparam:     WPARAM,
    lparam:     LPARAM,
) -> LRESULT {
    let mut result = LRESULT(0);

    let args = ProcArgs {
        win_handle,
        message,
        wparam,
        lparam
    };

    let do_subproc_mut = move |subproc: fn(&mut Window, ProcArgs)| unsafe {
        let prop = GetPropA(win_handle, s!("window_list"));
        let ptr = prop.0 as *const RefCell<WindowMap>;
        if prop.0 != 0 
        && let Some(state) = ptr.as_ref() 
        && let Ok(mut window_list) = state.try_borrow_mut()
        && let Some(window) = window_list.get_mut(&win_handle.0) {
            subproc(window, args); 
        } else {
            panic!("modify_window_state failed!");
        }
    };

    match message {
        WM_CREATE      => subprocedures::wm_create(args),
        WM_MOUSEMOVE   => do_subproc_mut(subprocedures::wm_mouse_move),
        WM_SIZE        => do_subproc_mut(subprocedures::wm_size),
        WM_LBUTTONDOWN => do_subproc_mut(subprocedures::wm_l_button_down),
        WM_LBUTTONUP   => do_subproc_mut(subprocedures::wm_l_button_up),
        WM_RBUTTONDOWN => do_subproc_mut(subprocedures::wm_r_button_down),
        WM_RBUTTONUP   => do_subproc_mut(subprocedures::wm_r_button_up),
        _ => result = DefWindowProcA(win_handle, message, wparam, lparam),
    }
    result
}
