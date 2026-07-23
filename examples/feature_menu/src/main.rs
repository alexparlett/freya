#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]
use freya::{
    menu::{
        Menu,
        MenuEvent,
        MenuItem,
        PredefinedMenuItem,
        Submenu,
    },
    prelude::*,
};

fn main() {
    let menu = || {
        let app_menu = Submenu::new("Freya", true);
        let _ = app_menu.append(&PredefinedMenuItem::about(None, None));
        let _ = app_menu.append(&PredefinedMenuItem::separator());
        let _ = app_menu.append(&PredefinedMenuItem::hide(None));
        let _ = app_menu.append(&PredefinedMenuItem::separator());
        // A custom Quit routed through the window's close request, so an `on_close`
        // hook keeps its veto — `PredefinedMenuItem::quit` would terminate directly.
        let _ = app_menu.append(&MenuItem::with_id(
            "quit",
            "Quit Freya",
            true,
            "CmdOrCtrl+Q".parse().ok(),
        ));
        let menu = Menu::new();
        let _ = menu.append(&app_menu);
        menu
    };
    let menu_handler = |MenuEvent { id }: MenuEvent, mut ctx: RendererContext| {
        if id == "quit" {
            ctx.request_close_window(None);
        }
    };
    launch(
        LaunchConfig::new()
            .with_menu(menu, menu_handler)
            .with_window(WindowConfig::new(app).with_size(500., 450.)),
    )
}

fn app() -> impl IntoElement {
    rect()
        .expanded()
        .center()
        .child(label().text("Try the menubar's Quit item (or ⌘Q)."))
}
