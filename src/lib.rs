#![feature(let_chains)]

// mod subprocedures;
use std::cell::RefCell;
use std::{collections::HashMap};
use std::pin::Pin;
use windows::{core::*, s, Win32::{Foundation::*, Graphics::Gdi::*, System::LibraryLoader::*, UI::WindowsAndMessaging::*, }, };
use common::geo::{Vector2, Rect2};

use std::sync::mpsc::{
    channel,
    Receiver,
    Sender
};

const CLASS_NAME: PCSTR = s!("RGUIWC");

#[derive(Debug)]
pub struct Window<'p> {
    platform       : &'p Platform,
    event_receiver : Receiver<WindowEvent>,
    /// ONLY to be utilized by the window procedure
    event_sender   : Pin<Box<Sender<WindowEvent>>>,
    handle         : HWND,
    device_context : HDC,
    pub minimized  : bool,
    pub mouse      : Mouse,
    pub size       : Rect2,
}

impl<'p> Window<'p> {
    pub fn new(platform: &'p Platform) -> Self {
        let window_name = s!("Rust GUI");
        let (tx, rx)    = channel::<WindowEvent>();
        let pinboxed_sender = Box::pin(tx);
        let handle  = unsafe {
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
                platform.process_handle,
                Some((pinboxed_sender.as_ref().get_ref() as *const Sender<WindowEvent>).cast()),
            )
        };
        
        Self {
            handle,
            platform,
            event_receiver : rx,
            event_sender   : pinboxed_sender,
            minimized      : false, 
            mouse          : Mouse::default(), 
            size           : Rect2::new(900, 600), 
            device_context : unsafe { GetDC(handle) }
        }
    }

    pub fn process_messages(&mut self) {
        let mut msg = MSG::default();
        unsafe {
            while PeekMessageA(&mut msg, self.handle, 0, 0, PM_REMOVE) != BOOL(0) {
                TranslateMessage(&mut msg);
                DispatchMessageA(&mut msg);
            }
        }
        while let Ok(event) = self.event_receiver.try_recv() {
            use WindowEvent::*;
            match event {
                MouseMoved {x,y} => {
                    self.mouse.pos.x = x;
                    self.mouse.pos.y = y;
                    println!("mousemove: {x}, {y}");
                },
                WindowResized {width, height} => {
                    self.size.height = height;
                    self.size.width = width;
                    println!("resized: {width}, {height}");
                },
                MouseButtonChanged(button, state) => {
                    use MouseButton::*;
                    use ButtonState::*;
                    match button {
                        Left if state == Up => self.mouse.left = false,
                        Left if state == Down => self.mouse.left = true,
                        Right if state == Up => self.mouse.right = false,
                        Right if state == Down => self.mouse.right = true,
                        _ => (),
                    }
                }
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
                        biSize        : std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth       : width as i32,
                        biHeight      : -(height as i32),
                        biPlanes      : 1,
                        biBitCount    : 32,
                        biCompression : BI_RGB,
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

/// Mouse state
#[derive(Debug, Copy, Clone)]
pub struct Mouse {
    pub pos   : Vector2,
    pub left  : bool,
    pub right : bool,
}

impl Default for Mouse {
    fn default() -> Self {
        Self {
            pos   : Vector2::new(0,0),
            left  : false,
            right : false,
        }
    }
}

#[derive(Debug)]
pub struct Platform {
    process_handle : HINSTANCE,
}

impl Platform {
    pub fn init() -> Self {
        let handle   = instance_handle();
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
        Self {
            process_handle : handle,
        }
    }

    pub fn create_window(&self) -> Window {
        Window::new(&self)
    }
}

fn instance_handle() -> HINSTANCE {
    unsafe { GetModuleHandleA(None).unwrap() }
}

unsafe extern "system" fn window_proc(
    win_handle: HWND,
    message:    u32,
    wparam:     WPARAM,
    lparam:     LPARAM,
) -> LRESULT {
    let mut result = LRESULT(0);

    let get_event_channel = || -> Option<Sender<WindowEvent>> {
        let prop = GetPropA(win_handle, s!("event_channel"));
        let ptr = prop.0 as *const Sender<WindowEvent>;
        ptr.as_ref().cloned()
    };

    // use subprocedures::*;

    use WindowEvent::*;
    use MouseButton::*;
    use ButtonState::*;

    match message {
        WM_CREATE => {
            let create_struct_ptr = lparam.0 as *const CREATESTRUCTA;
            if let Some(create_struct) = create_struct_ptr.as_ref() {
                SetPropA(win_handle, s!("event_channel"), HANDLE(create_struct.lpCreateParams as isize));
            }
        }
        WM_MOUSEMOVE => {
            if let Some(channel) = get_event_channel() {
                let lparam_bytes = lparam.0.to_le_bytes();
                let (x,y) = unsafe {
                    use std::mem::transmute;
                    (
                        transmute::<[u8; 2], u16>([lparam_bytes[0], lparam_bytes[1]]),
                        transmute::<[u8; 2], u16>([lparam_bytes[2], lparam_bytes[3]])
                    )
                };

                if let Err(e) = channel.send(WindowEvent::MouseMoved {
                    x: x as u32,
                    y: y as u32
                }) {
                    println!("{e}");
                    panic!();
                }
            }
        },
        WM_SIZE => {
            if let Some(channel) = get_event_channel() {
                let mut rect = RECT::default();
                unsafe {
                    GetClientRect(win_handle, &mut rect);
                }
                let width  = rect.right  as u32;
                let height = rect.bottom as u32;
                if let Err(e) = channel.send(WindowResized { width, height }) {
                    println!("{e}");
                    panic!()
                }
            }   
        }
        WM_LBUTTONDOWN => {
            if let Some(channel) = get_event_channel() {
                if let Err(e) = channel.send(MouseButtonChanged(
                    Left,
                    Down
                )) {
                    println!("{e}");
                    panic!();
                }
            }
        }
        WM_LBUTTONUP => {
            if let Some(channel) = get_event_channel() {
                if let Err(e) = channel.send(MouseButtonChanged(
                    Left,
                    Up
                )) {
                    println!("{e}");
                    panic!();
                }
            }
        }
        WM_RBUTTONDOWN => {
            if let Some(channel) = get_event_channel() {
                if let Err(e) = channel.send(MouseButtonChanged(
                    Right,
                    Down
                )) {
                    println!("{e}");
                    panic!();
                }
            }
        }
        WM_RBUTTONUP => {
            if let Some(channel) = get_event_channel() {
                if let Err(e) = channel.send(MouseButtonChanged(
                    Right,
                    Up
                )) {
                    println!("{e}");
                    panic!();
                }
            }
        }
        _ => result = DefWindowProcA(win_handle, message, wparam, lparam),
    }
    result
}

#[derive(Debug)]
enum WindowEvent {
    MouseMoved {
        x: u32,
        y: u32
    },
    MouseButtonChanged(MouseButton, ButtonState),
    WindowResized {
        width: u32,
        height: u32,
    }
}

#[derive(Debug)]
enum MouseButton {
    Left,
    Right,
    Middle,
    MB4,
    MB5
}

#[derive(Debug, PartialEq)]
enum ButtonState {
    Up,
    Down
}