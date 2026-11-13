use muda::{Menu, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

pub struct TrayHandle {
    _tray: tray_icon::TrayIcon,
    open_id: String,
    quit_id: String,
    port: u16,
}

fn app_icon() -> Icon {
    let mut rgba = Vec::with_capacity(32 * 32 * 4);
    for y in 0..32 {
        for x in 0..32 {
            let edge = x < 2 || y < 2 || x >= 30 || y >= 30;
            if edge {
                rgba.extend_from_slice(&[34, 197, 94, 255]);
            } else {
                rgba.extend_from_slice(&[15, 17, 21, 255]);
            }
        }
    }
    Icon::from_rgba(rgba, 32, 32).expect("tray icon rgba")
}

pub fn init(port: u16) -> Option<TrayHandle> {
    let menu = Menu::new();
    let open = MenuItem::with_id("open", "Open Remote Browser", true, None);
    let quit = MenuItem::with_id("quit", "Quit", true, None);
    if menu
        .append_items(&[&open, &PredefinedMenuItem::separator(), &quit])
        .is_err()
    {
        return None;
    }

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Remote Browser (left click: open, right click: menu)")
        .with_icon(app_icon())
        .build()
        .ok()?;

    // TrayIcon is !Send and its clicks arrive through a window message loop,
    // so the icon lives on the dedicated pump thread (see spawn).
    Some(TrayHandle {
        _tray: tray,
        open_id: open.id().0.clone(),
        quit_id: quit.id().0.clone(),
        port,
    })
}

fn open_browser(port: u16) {
    let url = format!("http://localhost:{port}");
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", &url])
        .spawn();
}

pub fn spawn(port: u16, quit: tokio::sync::oneshot::Sender<()>) {
    std::thread::spawn(move || {
        let handle = match init(port) {
            Some(h) => h,
            // Tray unavailable: keep the quit channel open so the server
            // keeps running headless instead of shutting down at once.
            None => {
                std::mem::forget(quit);
                return;
            }
        };
        pump(handle, quit);
    });
}

fn pump(handle: TrayHandle, quit: tokio::sync::oneshot::Sender<()>) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_QUIT,
    };

    unsafe {
        let mut msg = std::mem::zeroed::<MSG>();
        loop {
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                if msg.message == WM_QUIT {
                    return;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            while let Ok(ev) = muda::MenuEvent::receiver().try_recv() {
                if ev.id.0 == handle.open_id {
                    open_browser(handle.port);
                } else if ev.id.0 == handle.quit_id {
                    let _ = quit.send(());
                    return;
                }
            }

            while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
                if matches!(
                    ev,
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    }
                ) {
                    open_browser(handle.port);
                }
            }

            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}
