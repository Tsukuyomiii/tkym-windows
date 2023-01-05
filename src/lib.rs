use windows::{ core::*, s, Win32::{ Foundation::*, Graphics::Gdi::*, System::LibraryLoader::*, UI::WindowsAndMessaging::*, }, };

use common::geo::{Point, Rect2};

pub static mut WINDOW_LIST: WindowList = WindowList::new();

const CLASS_NAME: PCSTR = s!("RGUIWC");

fn instance_handle() -> HINSTANCE {
    unsafe { GetModuleHandleA(None).unwrap() }
}

#[derive(Debug)]
pub struct Window {
    pub handle: HWND,
    pub state: State,
    device_context: HDC,
}

impl Window {
    pub fn new(handle: HWND, state: State) -> Self {
        Self { handle, state, device_context: unsafe { GetDC(handle) } }
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

    pub fn swap_buffers(&self, buffer: *const u8) {
        let Rect2 { width, height } = self.state.size;
        unsafe {
            StretchDIBits(
                self.device_context,
                0,
                0,
                width as i32,
                height as i32,
                0,
                0,
                width as i32,
                height as i32,
                Some(buffer.cast()),
                &BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: width as i32,
                        biHeight: -(height as i32),
                        biPlanes: 1,
                        biBitCount: 32,
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

/// State for windows to be passed into and managed by the window procedure.
#[derive(Debug)]
pub struct State {
    /// Details of the mouse, as last captured by the window.
    pub mouse: Mouse,
    /// The current size of the window.
    pub size: Rect2,
}

impl Default for State {
    fn default() -> Self {
        Self {
            mouse: Mouse::default(),
            size: Rect2::new(900, 600)
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Mouse {
    /// The current mouse position in window coordinates.
    pub pos: Point,
    // Buttons
    pub left: bool,
    pub right: bool,
}

impl Default for Mouse {
    fn default() -> Self {
        Self {
            pos: Point::new(0,0),
            left: false,
            right: false,
        }
    }
}

#[derive(Debug)]
pub struct Platform {
    pub handle: HINSTANCE,
}

impl Platform {
    pub fn init() -> Self {
        let handle = instance_handle();
        unsafe {
            RegisterClassA(&WNDCLASSA {
                style: CS_HREDRAW | CS_HREDRAW | CS_OWNDC, 
                hInstance: handle,
                hCursor: HCURSOR(0),
                hIcon: HICON(0),
                lpszClassName: CLASS_NAME,
                lpfnWndProc: Some(window_proc),
                ..Default::default()
            });
        }
        Self { handle }
    }

    pub fn create_window(&self) -> WindowHandle {
        let window_name = s!("Rust GUI");
        let state = State::default();
        let win_handle = unsafe {
            CreateWindowExA(
                WINDOW_EX_STYLE(0),
                CLASS_NAME,
                window_name,
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                state.size.width  as i32,
                state.size.height as i32,
                HWND(0),
                HMENU(0),
                self.handle,
                None,
            )
        };
        let window_state = Window::new(win_handle, state);
        unsafe {
            WINDOW_LIST.0.push(window_state);
        };
        WindowHandle(win_handle)
    }
}

#[derive(Debug)]
pub struct WindowList(pub Vec<Window>);

impl WindowList {
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    #[allow(dead_code)]
    pub fn get_state<'a, T: Into<HWND> + Copy>(&'a self, handle: T) -> Option<&'a Window> {
        let mut result = None;
        for window in &self.0 {
            if window.handle.0 == handle.into().0 {
                result = Some(window);
            }
        };
        result
    }

    pub fn get_state_mut<'a, T: Into<HWND> + Copy>(&'a mut self, handle: T) -> Option<&'a mut Window> {
        let mut result = None;
        for window in &mut self.0 {
            if window.handle.0 == handle.into().0 {
                result = Some(window);
            }
        };
        result
    }
}

/// Interface for user to interact with created windows.
pub struct WindowHandle(HWND);

impl std::ops::Deref for WindowHandle {
    type Target = Window;
    fn deref(&self) -> &Self::Target {
        for window in unsafe { &WINDOW_LIST.0 } {
            if window.handle.0 == self.0.0 {
                return window;
            }
        }
        panic!("Deref<Window> for WindowHandle failed!");
    }
}

impl From<HWND> for WindowHandle {
    fn from(value: HWND) -> Self {
        Self(value)
    }
}

unsafe extern "system" fn window_proc(
    win_handle: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let mut result = LRESULT(0);
    match message {
        WM_MOUSEMOVE => if let Some(window) = WINDOW_LIST.get_state_mut(win_handle) {
            // optimize: would reading off a raw pointer be faster than byte splitting?
            let lparam_bytes = lparam.0.to_le_bytes();
            let x = std::mem::transmute::<[u8; 2], u16>([lparam_bytes[0], lparam_bytes[1]]);
            let y = std::mem::transmute::<[u8; 2], u16>([lparam_bytes[2], lparam_bytes[3]]);
            window.state.mouse.pos = Point::new(x as u32, y as u32);
        }
        WM_SIZE => if let Some(window) = WINDOW_LIST.get_state_mut(win_handle) {
            let mut rect = RECT::default();
            GetClientRect(win_handle, &mut rect);
            let width = rect.right as u32;
            let height = rect.bottom as u32;
            window.state.size = Rect2::new(width,height);
        }
        WM_LBUTTONDOWN => if let Some(window) = WINDOW_LIST.get_state_mut(win_handle) { window.state.mouse.left  = true },
        WM_LBUTTONUP   => if let Some(window) = WINDOW_LIST.get_state_mut(win_handle) { window.state.mouse.left  = false },
        WM_RBUTTONDOWN => if let Some(window) = WINDOW_LIST.get_state_mut(win_handle) { window.state.mouse.right = true },
        WM_RBUTTONUP   => if let Some(window) = WINDOW_LIST.get_state_mut(win_handle) { window.state.mouse.right = false },
        _ => result = DefWindowProcA(win_handle, message, wparam, lparam),
    }
    result
}
