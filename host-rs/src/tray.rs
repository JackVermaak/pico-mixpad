//! Notification-area (tray) icon: shows whether the pad is connected, opens the settings page,
//! and quits the app.

use std::time::Duration;

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use windows::Win32::UI::WindowsAndMessaging::{DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE};

use crate::actions::open_target;
use crate::status::Shared;

const SIZE: u32 = 32;

/// A small knob with a status dot: green when the pad is connected, amber when it isn't.
fn icon(connected: bool) -> Icon {
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    let dot = if connected { [61, 220, 132] } else { [240, 160, 48] };
    let blend = |px: &mut [u8], rgb: [u8; 3], cover: f32| {
        if cover <= 0.0 {
            return;
        }
        let a = cover.min(1.0);
        let old_a = px[3] as f32 / 255.0;
        let out_a = a + old_a * (1.0 - a);
        for i in 0..3 {
            let mixed = (rgb[i] as f32 * a + px[i] as f32 * old_a * (1.0 - a)) / out_a.max(1e-6);
            px[i] = mixed.round() as u8;
        }
        px[3] = (out_a * 255.0).round() as u8;
    };
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let px = &mut rgba[((y * SIZE + x) * 4) as usize..][..4];
            // knob body
            let d = ((fx - 14.0).powi(2) + (fy - 14.0).powi(2)).sqrt();
            blend(px, [210, 210, 214], 13.5 - d);
            blend(px, [40, 41, 46], 12.0 - d);
            // pointer line from the centre toward the top
            if (fx - 14.0).abs() < 1.6 && fy > 4.0 && fy < 12.0 {
                blend(px, [235, 235, 238], 1.0);
            }
            // status dot with a dark ring
            let s = ((fx - 25.0).powi(2) + (fy - 25.0).powi(2)).sqrt();
            blend(px, [20, 20, 22], 7.0 - s);
            blend(px, dot, 5.5 - s);
        }
    }
    Icon::from_rgba(rgba, SIZE, SIZE).expect("valid icon")
}

fn describe(status: &Shared) -> (bool, String) {
    let st = status.lock();
    if st.connected {
        let fw = st.firmware.as_deref().map(|f| format!(", firmware {f}")).unwrap_or_default();
        (true, format!("Pad connected on {}{fw}", st.port.as_deref().unwrap_or("?")))
    } else {
        (false, "Pad not connected".to_string())
    }
}

/// Runs the tray icon on the calling thread. Returns when the user chooses Quit.
pub fn run(status: Shared, url: &str) {
    let state_item = MenuItem::new("Pad not connected", false, None);
    let open_item = MenuItem::new("Open settings", true, None);
    let quit_item = MenuItem::new("Quit Pico Mixpad", true, None);
    let menu = Menu::new();
    let _ = menu.append_items(&[&state_item, &PredefinedMenuItem::separator(), &open_item, &quit_item]);

    let tray = match TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip("Pico Mixpad: pad not connected")
        .with_icon(icon(false))
        .build()
    {
        Ok(t) => t,
        Err(e) => {
            log::error!("tray icon unavailable: {e}; running without it");
            loop {
                std::thread::sleep(Duration::from_secs(3600));
            }
        }
    };

    let mut shown: Option<(bool, String)> = None;
    loop {
        // The tray icon talks to Windows through this thread's message queue.
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == *open_item.id() {
                open_target(url);
            } else if ev.id == *quit_item.id() {
                return;
            }
        }
        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                open_target(url);
            }
        }
        let now = describe(&status);
        if shown.as_ref() != Some(&now) {
            let _ = tray.set_icon(Some(icon(now.0)));
            let _ = tray.set_tooltip(Some(format!("Pico Mixpad: {}", now.1.to_lowercase())));
            state_item.set_text(&now.1);
            shown = Some(now);
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}
