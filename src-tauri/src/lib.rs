use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager,
};

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn show_main_window(window: tauri::Window) {
    window.show().unwrap();
    window.set_focus().unwrap();
}

#[tauri::command]
fn hide_main_window(window: tauri::Window) {
    if window.is_visible().unwrap() {
        window.hide().unwrap();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            #[cfg(desktop)]
            {
                use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

                // Create Cmd+. shortcut
                let cmd_dot_shortcut = Shortcut::new(Some(Modifiers::SUPER), Code::Period);
                
                app.handle().plugin(
                    tauri_plugin_global_shortcut::Builder::new().with_handler(move |_app, shortcut, event| {
                        if shortcut == &cmd_dot_shortcut {
                            match event.state() {
                                ShortcutState::Pressed => {
                                    println!("Cmd+. Pressed!");
                                    // Show and focus the main window
                                    if let Some(window) = _app.get_webview_window("main") {
                                        if window.is_visible().unwrap() {
                                            window.hide().unwrap();
                                        } else {
                                            window.show().unwrap();
                                            window.set_focus().unwrap();
                                        }
                                    }
                                }
                                ShortcutState::Released => {
                                    println!("Cmd+. Released!");
                                }
                            }
                        }
                    })
                    .build(),
                )?;

                app.global_shortcut().register(cmd_dot_shortcut)?;

                // Create tray menu
                let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&quit_i])?;

                // Create tray icon
                let _tray = TrayIconBuilder::new()
                    .icon(app.default_window_icon().unwrap().clone())
                    .menu(&menu)
                    .show_menu_on_left_click(true)
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "quit" => {
                            println!("quit menu item was clicked");
                            app.exit(0);
                        }
                        _ => {
                            println!("menu item {:?} not handled", event.id);
                        }
                    })
                    .build(app)?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet, show_main_window, hide_main_window])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
