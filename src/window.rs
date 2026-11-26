//! Waddle's own window and its tray icon. Closing the window leaves the servers running;
//! only Quit, in the tray or in Settings, stops them.

use crate::service::Service;
use crate::system::PNG;
use std::sync::Arc;
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::{Window, WindowBuilder};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use wry::{WebContext, WebView, WebViewBuilder};

enum Wake {
    Show,
    Quit,
    Login,
    Tray,
}

/// Whether closing the window leaves Waddle running in the tray. Off unless chosen: closing quits.
pub fn keep_in_tray(ctx: &crate::util::Ctx) -> bool {
    crate::config::ui_flag(&crate::config::load_ui(ctx), "close_to_tray")
}

/// The penguin as RGBA pixels.
fn penguin() -> (Vec<u8>, u32, u32) {
    let mut reader = png::Decoder::new(PNG).read_info().expect("the icon is a PNG");
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).expect("the icon is a PNG");
    pixels.truncate(info.buffer_size());
    (pixels, info.width, info.height)
}

fn open(target: &tao::event_loop::EventLoopWindowTarget<Wake>, context: &mut WebContext, url: &str) -> Option<(Window, WebView)> {
    let (pixels, width, height) = penguin();
    let window = WindowBuilder::new()
        .with_title("Waddle")
        .with_inner_size(tao::dpi::LogicalSize::new(1480.0, 920.0))
        .with_min_inner_size(tao::dpi::LogicalSize::new(420.0, 480.0))
        .with_window_icon(tao::window::Icon::from_rgba(pixels, width, height).ok())
        .build(target)
        .ok()?;
    // Waddle's window only ever shows Waddle. Links to other sites open in the person's own browser.
    let home = url.split("/?").next().unwrap_or(url).to_string();
    let builder = WebViewBuilder::with_web_context(context)
        .with_url(url)
        .with_devtools(cfg!(debug_assertions))
        .with_navigation_handler(move |to| {
            if to.starts_with(&home) || to.starts_with("data:") || to == "about:blank" {
                return true;
            }
            crate::open_external(&to);
            false
        })
        .with_new_window_req_handler(|to| {
            crate::open_external(&to);
            false
        });
    #[cfg(not(target_os = "linux"))]
    let webview = builder.build(&window);
    #[cfg(target_os = "linux")]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        builder.build_gtk(window.default_vbox()?)
    };
    match webview {
        Ok(webview) => Some((window, webview)),
        Err(_) => None,
    }
}

pub fn run(service: Arc<Service>, show_now: bool) -> ! {
    let event_loop = EventLoopBuilder::<Wake>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let ctx = service.ctx.clone();
    let url = crate::dashboard_url(&ctx);
    let mut context = WebContext::new(Some(ctx.logs().join("window")));

    {
        let proxy = std::sync::Mutex::new(proxy.clone());
        *service.show.lock().unwrap() = Some(Box::new(move || {
            let _ = proxy.lock().unwrap().send_event(Wake::Show);
        }));
    }
    let menu = Menu::new();
    let show_item = MenuItem::new("Open Waddle", true, None);
    let quit_item = MenuItem::new("Quit Waddle", true, None);
    let login_item = tray_icon::menu::CheckMenuItem::new("Start at sign-in", true, crate::system::startup_on(&ctx), None);
    let tray_item = tray_icon::menu::CheckMenuItem::new("Keep running when closed", true, keep_in_tray(&ctx), None);
    let _ = menu.append_items(&[&show_item, &PredefinedMenuItem::separator(), &login_item, &tray_item, &PredefinedMenuItem::separator(), &quit_item]);
    let (show_id, quit_id) = (show_item.id().clone(), quit_item.id().clone());
    let (login_id, tray_id) = (login_item.id().clone(), tray_item.id().clone());

    {
        let proxy = proxy.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let wake = if event.id == show_id {
                Wake::Show
            } else if event.id == quit_id {
                Wake::Quit
            } else if event.id == login_id {
                Wake::Login
            } else if event.id == tray_id {
                Wake::Tray
            } else {
                return;
            };
            let _ = proxy.send_event(wake);
        }));
    }
    {
        let proxy = proxy.clone();
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            let opened = match event {
                TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } => true,
                TrayIconEvent::DoubleClick { .. } => true,
                _ => false,
            };
            if opened {
                let _ = proxy.send_event(Wake::Show);
            }
        }));
    }

    let mut tray = None;
    let mut window: Option<(Window, WebView)> = None;
    event_loop.run(move |event, target, control_flow| {
        // wake every few seconds, so the tray's switches show changes made in Settings
        *control_flow = ControlFlow::WaitUntil(std::time::Instant::now() + std::time::Duration::from_secs(10));
        if matches!(event, Event::NewEvents(StartCause::ResumeTimeReached { .. }) | Event::UserEvent(Wake::Login | Wake::Tray)) {
            login_item.set_checked(crate::system::startup_on(&ctx) ^ matches!(event, Event::UserEvent(Wake::Login)));
            tray_item.set_checked(keep_in_tray(&ctx) ^ matches!(event, Event::UserEvent(Wake::Tray)));
        }
        match event {
            Event::NewEvents(StartCause::Init) => {
                // the tray needs the event loop running on some systems, so it's made here
                let (pixels, width, height) = penguin();
                tray = TrayIconBuilder::new()
                    .with_tooltip("Waddle")
                    .with_menu(Box::new(menu.clone()))
                    .with_menu_on_left_click(false)
                    .with_icon(tray_icon::Icon::from_rgba(pixels, width, height).expect("the icon fits"))
                    .build()
                    .ok();
                if show_now || tray.is_none() {
                    window = open(target, &mut context, &url);
                    if window.is_none() {
                        crate::open_browser(&ctx); // no WebView2 or WebKit here
                    }
                }
            }
            Event::UserEvent(Wake::Show) => match &window {
                Some((win, _)) => {
                    win.set_visible(true);
                    win.set_minimized(false);
                    win.set_focus();
                }
                None => {
                    window = open(target, &mut context, &url);
                    if window.is_none() {
                        crate::open_browser(&ctx);
                    }
                }
            },
            Event::UserEvent(Wake::Quit) => {
                window = None;
                tray = None;
                let service = service.clone();
                std::thread::spawn(move || {
                    service.shutdown();
                    std::process::exit(0);
                });
            }
            Event::WindowEvent { event: WindowEvent::Focused(on), .. } => {
                service.focused.store(on, std::sync::atomic::Ordering::SeqCst);
            }
            Event::UserEvent(Wake::Login) => {
                let _ = crate::system::set_startup(&ctx, !crate::system::startup_on(&ctx));
            }
            Event::UserEvent(Wake::Tray) => {
                let on = !keep_in_tray(&ctx);
                let _ = crate::config::save_ui(&ctx, [("close_to_tray".to_string(), serde_json::json!(on))].into_iter().collect());
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                window = None;
                service.focused.store(false, std::sync::atomic::Ordering::SeqCst);
                // closing quits everything: every model, the image server, and every process Waddle started,
                // unless the person chose to keep it running in the tray
                if tray.is_none() || !keep_in_tray(&ctx) {
                    tray = None;
                    let service = service.clone();
                    std::thread::spawn(move || {
                        service.shutdown();
                        std::process::exit(0);
                    });
                }
            }
            _ => {}
        }
    })
}
