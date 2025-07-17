use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager,
};
use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use chrono::{DateTime, Local};

#[derive(Debug, Serialize, Deserialize)]
struct Task {
    id: String,
    created_at: DateTime<Local>,
    task_description: String,
}

fn get_worklog_dir() -> Result<PathBuf, String> {
    let home_dir = std::env::var("HOME").map_err(|_| "Could not get home directory")?;
    let worklog_dir = PathBuf::from(home_dir).join(".worklog");
    
    // Create directory if it doesn't exist
    create_dir_all(&worklog_dir).map_err(|e| format!("Could not create worklog directory: {}", e))?;
    
    Ok(worklog_dir)
}

#[tauri::command]
fn save_task(task: String) -> Result<String, String> {
    let worklog_dir = get_worklog_dir()?;
    let csv_path = worklog_dir.join("tasks.csv");
    
    // Check if CSV file exists, if not create it with headers
    let file_exists = csv_path.exists();
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&csv_path)
        .map_err(|e| format!("Could not open CSV file: {}", e))?;
    
    // Write headers if file is new
    if !file_exists {
        writeln!(file, "id,created_at,task_description")
            .map_err(|e| format!("Could not write CSV headers: {}", e))?;
    }
    
    // Create task entry
    let task_entry = Task {
        id: uuid::Uuid::new_v4().to_string(),
        created_at: Local::now(),
        task_description: task.clone(),
    };
    
    // Write CSV row
    writeln!(
        file,
        "{},\"{}\",\"{}\"",
        task_entry.id,
        task_entry.created_at.format("%Y-%m-%d %H:%M:%S"),
        task_entry.task_description.replace("\"", "\"\"") // Escape quotes in task
    ).map_err(|e| format!("Could not write CSV row: {}", e))?;
    
    Ok(format!("Task saved successfully: {}", task))
}

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
        .invoke_handler(tauri::generate_handler![greet, show_main_window, hide_main_window, save_task])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
