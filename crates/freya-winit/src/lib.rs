pub mod reexports {
    pub use winit;
}

use std::sync::Arc;

use freya_core::{
    integration::GlobalContexts,
    prelude::{
        GlobalTasks,
        Platform,
    },
};

use crate::{
    config::LaunchConfig,
    drivers::GraphicsContext,
    renderer::{
        LaunchHandle,
        RendererAppMessage,
        RendererMessage,
        WinitRenderer,
    },
};
mod accessibility;
mod clipboard;
pub mod config;
mod data_transfer;
mod drag_preview;
mod drivers;
pub mod extensions;
pub mod integration;
mod parent_window;
mod platform_drag;
pub mod plugins;
pub mod renderer;
#[cfg(target_os = "macos")]
mod traffic_light;
#[cfg(feature = "tray")]
mod tray_icon;
mod window;
mod winit_mappings;

pub use extensions::*;
use futures_util::task::{
    ArcWake,
    waker,
};

use crate::renderer::RendererSender;

pub mod winit {
    pub use winit::*;
}

#[cfg(feature = "tray")]
pub mod tray {
    pub use tray_icon::*;

    pub use crate::tray_icon::*;
}

/// The muda menu vocabulary for building an application menubar
/// (see [`LaunchConfig::with_menu`](crate::config::LaunchConfig::with_menu)).
#[cfg(feature = "menu")]
pub mod menu {
    pub use muda::*;
}

/// Launch the application.
///
/// If a custom event loop was provided via [`LaunchConfig::with_event_loop`], it will be used.
/// Otherwise a default one is created.
pub fn launch(launch_config: LaunchConfig) {
    #[cfg(all(not(debug_assertions), not(target_os = "android")))]
    {
        let run_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            launch_inner(launch_config);
        }));
        if let Err(panic_payload) = run_result {
            let description = panic_payload
                .downcast_ref::<&str>()
                .map(|message| message.to_string())
                .or_else(|| panic_payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "The application panicked.".to_string());
            rfd::MessageDialog::new()
                .set_title("Fatal Error")
                .set_description(&description)
                .set_level(rfd::MessageLevel::Error)
                .show();
            std::process::exit(1);
        }
    }

    #[cfg(any(debug_assertions, target_os = "android"))]
    launch_inner(launch_config);
}

fn launch_inner(mut launch_config: LaunchConfig) {
    use std::collections::HashMap;

    use freya_engine::prelude::{
        FontCollection,
        FontMgr,
        SkData,
        TypefaceFontProvider,
        register_font_typeface,
    };
    use winit::event_loop::EventLoop;

    let event_loop = launch_config.event_loop.take().unwrap_or_else(|| {
        EventLoop::builder()
            .build()
            .expect("Failed to create event loop.")
    });

    let (sender, receiver) = std::sync::mpsc::channel();
    let proxy = RendererSender::new(event_loop.create_proxy(), sender);

    let mut font_collection = FontCollection::new();
    let def_mgr = FontMgr::default();
    let font_mgr = FontMgr::custom_empty().unwrap_or_default();
    let mut provider = TypefaceFontProvider::new();
    for (font_name, font_data) in &launch_config.embedded_fonts {
        let typeface = font_mgr
            .new_from_data(SkData::new_copy(font_data), None)
            .unwrap_or_else(|| panic!("Failed to load font {font_name}."));
        register_font_typeface(&mut provider, font_name.as_ref(), typeface);
    }
    let font_mgr: FontMgr = provider.clone().into();
    font_collection.set_default_font_manager(def_mgr, None);
    font_collection.set_dynamic_font_manager(font_mgr.clone());
    font_collection.paragraph_cache_mut().turn_on(false);

    struct FuturesWaker(RendererSender);

    impl ArcWake for FuturesWaker {
        fn wake_by_ref(arc_self: &Arc<Self>) {
            _ = arc_self
                .0
                .send_message(RendererMessage::App(RendererAppMessage::PollFutures));
        }
    }

    let waker = waker(Arc::new(FuturesWaker(proxy.clone())));

    #[cfg(feature = "hotreload")]
    freya_core::hotreload::connect_subsecond();

    let global_contexts = GlobalContexts::register();
    global_contexts.insert_context(Platform::new({
        let proxy = proxy.clone();
        move |event| {
            let _ = proxy.send_message(RendererMessage::App(RendererAppMessage::Command(event)));
        }
    }));
    global_contexts.insert_context(crate::clipboard::create_clipboard(proxy.clone()));
    for insert_global in launch_config.globals {
        insert_global(&global_contexts);
    }

    let global_tasks = global_contexts.insert_context(GlobalTasks::new(waker));
    for task in launch_config.tasks {
        global_tasks.spawn(task(LaunchHandle(proxy.clone())));
    }

    let renderer = WinitRenderer {
        windows: HashMap::default(),
        global_contexts,
        #[cfg(feature = "tray")]
        tray: launch_config.tray,
        #[cfg(feature = "tray")]
        tray_icon: None,
        #[cfg(feature = "menu")]
        menu: launch_config.menu,
        #[cfg(feature = "menu")]
        app_menu: None,
        resumed: false,
        proxy,
        messages: receiver,
        clipboard_reads: HashMap::default(),
        font_provider: provider,
        font_manager: font_mgr,
        font_collection,
        windows_configs: launch_config.windows_configs,
        plugins: launch_config.plugins,
        fallback_fonts: launch_config.fallback_fonts,
        exit_on_close: launch_config.exit_on_close,
        gpu_resource_cache_limit: launch_config.gpu_resource_cache_limit,
        graphics_context: GraphicsContext::default(),
    };

    #[cfg(feature = "tray")]
    {
        #[cfg(not(feature = "menu"))]
        use crate::tray::menu::MenuEvent;
        use crate::tray::{
            TrayEvent,
            TrayIconEvent,
        };

        // With the `menu` feature on, muda's single global handler is set below and the
        // renderer fans menu events out to both the menubar and tray handlers.
        #[cfg(not(feature = "menu"))]
        {
            let proxy = renderer.proxy.clone();
            MenuEvent::set_event_handler(Some(move |event| {
                let _ = proxy.send_message(RendererMessage::Tray(TrayEvent::Menu(event)));
            }));
        }
        let proxy = renderer.proxy.clone();
        TrayIconEvent::set_event_handler(Some(move |event| {
            let _ = proxy.send_message(RendererMessage::Tray(TrayEvent::Icon(event)));
        }));
    }

    // Muda's single global menu handler: forward every menu event through the loop; the
    // renderer fans it out to the menubar handler and, when enabled, the tray handler.
    #[cfg(feature = "menu")]
    {
        let proxy = renderer.proxy.clone();
        muda::MenuEvent::set_event_handler(Some(move |event| {
            let _ = proxy.send_message(RendererMessage::Menu(event));
        }));
    }

    // The loop ends in an error when the display goes away, as at the end of a session.
    if let Err(err) = event_loop.run_app(renderer) {
        tracing::warn!("event loop ended: {err}");
    }
}
