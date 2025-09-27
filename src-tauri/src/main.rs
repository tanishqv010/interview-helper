#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use genai::chat::{ChatMessage, ChatRequest, ContentPart};
use genai::Client;
use tauri::Manager;
use reqwest::Client as HttpClient;
use serde_json::json;

use base64::{engine::general_purpose, Engine as _};
use image::{ImageBuffer, Rgba};
use screenshots::Screen;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::sync::Arc;
use uuid::Uuid;

use tauri::{LogicalPosition, LogicalSize, PhysicalPosition, Position, Size};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use std::collections::VecDeque;

#[cfg(windows)]
use winreg::{enums::HKEY_CURRENT_USER, RegKey};
#[cfg(windows)]
use windows::Win32::Foundation::{LPARAM, WPARAM};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE};
#[cfg(windows)]
use windows::core::w;

struct ToggleState {
    visible: AtomicBool,
    last_toggle: Mutex<Instant>,
    last_nudge: Mutex<Instant>,
}

struct ImageQueue {
    images: Mutex<VecDeque<String>>,
    last_capture: Mutex<Instant>,
}

struct AppConfig {
    api_key: Mutex<Option<String>>, // Stored for reference; env var is also set
    model: Mutex<String>,
    hf_token: Mutex<Option<String>>, // Hugging Face token for GPT-OSS-120B
}

#[tauri::command]
fn move_window(position: &str, app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let screen = window.primary_monitor().unwrap().unwrap();
        let screen_size = screen.size();

        let screen_width = screen_size.width as f64;
        let screen_height = screen_size.height as f64;

        let (x, y) = match position {
            "top-left" => (0.0, 0.0),
            "top-right" => (screen_width - window.outer_size().unwrap().width as f64, 0.0),
            "bottom-left" => (0.0, screen_height - window.outer_size().unwrap().height as f64),
            "bottom-right" => (
                screen_width - window.outer_size().unwrap().width as f64,
                screen_height - window.outer_size().unwrap().height as f64,
            ),
            "center" => (
                (screen_width - window.outer_size().unwrap().width as f64) / 2.0,
                (screen_height - window.outer_size().unwrap().height as f64) / 2.0,
            ),
            _ => (100.0, 100.0),
        };
        window
            .set_position(Position::Logical(LogicalPosition { x, y }))
            .unwrap();
    }
}

#[tauri::command]
fn nudge_window(state: tauri::State<ToggleState>, direction: &str, step: i32, app: tauri::AppHandle) {
    // Debounce arrow holds and duplicate firings: allow nudges every 120ms
    {
        let mut last = state.last_nudge.lock().unwrap();
        let now = Instant::now();
        if now.duration_since(*last) < Duration::from_millis(120) {
            return;
        }
        *last = now;
    }
    if let Some(window) = app.get_webview_window("main") {
        if let Ok(current_pos) = window.outer_position() {
            let mut new_x = current_pos.x;
            let mut new_y = current_pos.y;

            let delta = if step == 0 { 50 } else { step };

            match direction {
                "up" => new_y -= delta,
                "down" => new_y += delta,
                "left" => new_x -= delta,
                "right" => new_x += delta,
                _ => {}
            }

            let _ = window.set_position(Position::Physical(PhysicalPosition { x: new_x, y: new_y }));
        }
    }
}

#[tauri::command]
fn toggle_window_visibility(state: tauri::State<ToggleState>, app: tauri::AppHandle) -> bool {
    // Debounce rapid repeats from key auto-repeat: allow only every 350ms
    {
        let mut last = state.last_toggle.lock().unwrap();
        let now = Instant::now();
        if now.duration_since(*last) < Duration::from_millis(350) {
            // Return current state without changing
            return state.visible.load(Ordering::SeqCst);
        }
        *last = now;
    }

    // Flip the app-level visibility flag and apply it to the window
    let was_visible = state.visible.fetch_xor(true, Ordering::SeqCst);
    let now_visible = !was_visible;

    if let Some(window) = app.get_webview_window("main") {
        if now_visible {
            // Restore original window properties to initial state
            let _ = window.set_always_on_top(true);
            let _ = window.set_decorations(false);
            let _ = window.set_content_protected(true);
            let _ = window.set_skip_taskbar(true);
            let _ = window.set_ignore_cursor_events(false);
        } else {
            // Change state to hidden without hiding window
            let _ = window.set_ignore_cursor_events(true);
        }
    }

    now_visible
}

#[tauri::command]
fn resize_window(width: f64, height: f64, app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_size(Size::Logical(LogicalSize { width, height }));
    }
}

#[tauri::command]
fn capture_area(x: i32, y: i32, width: u32, height: u32) -> Result<String, String> {
    let screens = Screen::all().map_err(|e| e.to_string())?;
    let screen = screens.get(0).ok_or("No screens found")?;

    let image = screen
        .capture_area(x, y, width, height)
        .map_err(|e| e.to_string())?;

    let buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, image.into_raw())
        .ok_or("Failed to convert image")?;

    let filename = format!("{}.png", Uuid::new_v4());
    let mut path = std::env::temp_dir();
    path.push("tauri_gemini");
    fs::create_dir_all(&path).ok();
    path.push(filename);

    buffer.save(&path).map_err(|e| e.to_string())?;

    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
fn capture_full_screen() -> Result<String, String> {
    let binding = Screen::all().map_err(|e| e.to_string())?;
    let screen = binding.get(0).ok_or("No screens found")?;

    let image = screen.capture().map_err(|e| e.to_string())?;
    let (width, height) = (image.width(), image.height());

    let buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, image.into_raw())
        .ok_or("Failed to convert image")?;

    let filename = format!("{}.png", Uuid::new_v4());
    let mut path = std::env::temp_dir();
    path.push("tauri_gemini");
    fs::create_dir_all(&path).ok();
    path.push(filename);

    buffer.save(&path).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
fn set_gemini_api_key(key: String, cfg: tauri::State<'_, AppConfig>) -> Result<(), String> {
    // Store in memory and set environment for underlying client
    {
        let mut api_key_guard = cfg.api_key.lock().map_err(|_| "Lock poisoned")?;
        *api_key_guard = if key.trim().is_empty() { None } else { Some(key.clone()) };
    }
    if key.trim().is_empty() {
        std::env::remove_var("GEMINI_API_KEY");
    } else {
        std::env::set_var("GEMINI_API_KEY", key);
    }

    // Persist to Windows user environment and broadcast change
    #[cfg(windows)]
    {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(env) = hkcu.open_subkey_with_flags("Environment", winreg::enums::KEY_SET_VALUE) {
            if let Err(e) = env.set_value("GEMINI_API_KEY", &std::env::var("GEMINI_API_KEY").unwrap_or_default()) {
                eprintln!("Failed to write GEMINI_API_KEY to registry: {}", e);
            }
        }

        unsafe {
            let _ = SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM(0),
                LPARAM(w!("Environment").as_ptr() as isize),
                SMTO_ABORTIFHUNG,
                5000,
                None,
            );
        }
    }
    Ok(())
}

#[tauri::command]
fn get_gemini_api_key(cfg: tauri::State<'_, AppConfig>) -> Option<String> {
    if let Ok(guard) = cfg.api_key.lock() {
        return guard.clone();
    }
    None
}

#[tauri::command]
fn set_model(model: String, cfg: tauri::State<'_, AppConfig>) -> Result<String, String> {
    let mut model_guard = cfg.model.lock().map_err(|_| "Lock poisoned")?;
    *model_guard = model.clone();
    Ok(model)
}

#[tauri::command]
fn set_hf_token(token: String, cfg: tauri::State<'_, AppConfig>) -> Result<(), String> {
    let mut hf_token_guard = cfg.hf_token.lock().map_err(|_| "Lock poisoned")?;
    if token.is_empty() {
        *hf_token_guard = None;
        std::env::remove_var("HUGGINGFACE_TOKEN");
    } else {
        *hf_token_guard = Some(token.clone());
        std::env::set_var("HUGGINGFACE_TOKEN", token.clone());
    }

    // Persist to Windows user environment and broadcast change
    #[cfg(windows)]
    {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(env) = hkcu.open_subkey_with_flags("Environment", winreg::enums::KEY_SET_VALUE) {
            if let Err(e) = env.set_value("HUGGINGFACE_TOKEN", &std::env::var("HUGGINGFACE_TOKEN").unwrap_or_default()) {
                eprintln!("Failed to write HUGGINGFACE_TOKEN to registry: {}", e);
            }
        }

        unsafe {
            let _ = SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM(0),
                LPARAM(w!("Environment").as_ptr() as isize),
                SMTO_ABORTIFHUNG,
                5000,
                None,
            );
        }
    }
    Ok(())
}

#[tauri::command]
fn get_hf_token(cfg: tauri::State<'_, AppConfig>) -> Option<String> {
    if let Ok(guard) = cfg.hf_token.lock() {
        return guard.clone();
    }
    None
}

#[tauri::command]
async fn call_gemini(prompt: String, cfg: tauri::State<'_, AppConfig>) -> Result<String, String> {
    if std::env::var("GEMINI_API_KEY").is_err() {
        return Err("GEMINI_API_KEY environment variable not set.".to_string());
    }

    let client = Client::default();

    let chat_req = ChatRequest::new(vec![
        ChatMessage::system("Be concise and helpful."),
        ChatMessage::user(&prompt),
    ]);

    let model = cfg.model.lock().map_err(|_| "Lock poisoned")?.clone();

    let res = client
        .exec_chat(&model, chat_req, None)
        .await
        .map_err(|e| e.to_string())?;

    Ok(res
        .content_text_as_str()
        .unwrap_or("[No response]")
        .to_string())
}

#[tauri::command]
async fn call_gemini_with_image(prompt: String, image_path: String, cfg: tauri::State<'_, AppConfig>) -> Result<String, String> {
    if std::env::var("GEMINI_API_KEY").is_err() {
        return Err("GEMINI_API_KEY environment variable not set.".to_string());
    }

    let mut file = File::open(&image_path).map_err(|e| e.to_string())?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).map_err(|e| e.to_string())?;

    let encoded_image = general_purpose::STANDARD.encode(&buffer);

    let client = Client::default();

    let chat_req = ChatRequest::new(vec![
        ChatMessage::system("Be concise and helpful."),
        ChatMessage::user(vec![
            ContentPart::from_text(prompt),
            ContentPart::from_image_base64("image/png", Arc::from(encoded_image)),
        ]),
    ]);

    let model = cfg.model.lock().map_err(|_| "Lock poisoned")?.clone();

    let res = client
        .exec_chat(&model, chat_req, None)
        .await
        .map_err(|e| e.to_string())?;

    Ok(res
        .content_text_as_str()
        .unwrap_or("[No response]")
        .to_string())
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn add_image_to_queue(queue: tauri::State<'_, ImageQueue>) -> Result<usize, String> {
    // Debounce: only allow one capture per 500ms
    {
        let mut last_capture = queue.last_capture.lock().unwrap();
        let now = Instant::now();
        if now.duration_since(*last_capture) < Duration::from_millis(500) {
            // Return current queue length without adding new image
            let images = queue.images.lock().unwrap();
            return Ok(images.len());
        }
        *last_capture = now;
    }

    let screens = Screen::all().map_err(|e| e.to_string())?;
    let screen = screens.get(0).ok_or("No screens found")?;

    let image = screen.capture().map_err(|e| e.to_string())?;
    let (width, height) = (image.width(), image.height());

    let buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, image.into_raw())
        .ok_or("Failed to convert image")?;

    let filename = format!("{}.png", Uuid::new_v4());
    let mut path = std::env::temp_dir();
    path.push("tauri_gemini");
    fs::create_dir_all(&path).ok();
    path.push(filename);

    buffer.save(&path).map_err(|e| e.to_string())?;

    let mut images = queue.images.lock().unwrap();
    images.push_back(path.to_string_lossy().to_string());
    Ok(images.len())
}

#[tauri::command]
fn get_queue_length(queue: tauri::State<'_, ImageQueue>) -> usize {
    let images = queue.images.lock().unwrap();
    images.len()
}

#[tauri::command]
fn clear_queue(queue: tauri::State<'_, ImageQueue>) {
    let mut images = queue.images.lock().unwrap();
    images.clear();
}

#[tauri::command]
async fn call_gemini_with_image_queue(prompt: String, queue: tauri::State<'_, ImageQueue>, cfg: tauri::State<'_, AppConfig>) -> Result<String, String> {
    if std::env::var("GEMINI_API_KEY").is_err() {
        return Err("GEMINI_API_KEY environment variable not set.".to_string());
    }

    // Collect image paths and release the lock before async operations
    let image_paths = {
        let images = queue.images.lock().unwrap();
        if images.is_empty() {
            return Err("No images in queue".to_string());
        }
        images.iter().cloned().collect::<Vec<String>>()
    };

    let client = Client::default();
    let mut content_parts = vec![ContentPart::from_text(prompt)];

    // Add all images from the queue
    for image_path in image_paths.iter() {
        let mut file = File::open(image_path).map_err(|e| e.to_string())?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).map_err(|e| e.to_string())?;

        let encoded_image = general_purpose::STANDARD.encode(&buffer);
        content_parts.push(ContentPart::from_image_base64("image/png", Arc::from(encoded_image)));
    }

    let chat_req = ChatRequest::new(vec![
        ChatMessage::system("Be concise and helpful. Analyze all provided images in order."),
        ChatMessage::user(content_parts),
    ]);

    let model = cfg.model.lock().map_err(|_| "Lock poisoned")?.clone();

    let res = client
        .exec_chat(&model, chat_req, None)
        .await
        .map_err(|e| e.to_string())?;

    Ok(res
        .content_text_as_str()
        .unwrap_or("[No response]")
        .to_string())
}

#[tauri::command]
async fn call_beast_mode(prompt: String, queue: tauri::State<'_, ImageQueue>, cfg: tauri::State<'_, AppConfig>) -> Result<String, String> {
    if std::env::var("GEMINI_API_KEY").is_err() {
        return Err("GEMINI_API_KEY environment variable not set.".to_string());
    }

    // Collect image paths and release the lock before async operations
    let image_paths = {
        let images = queue.images.lock().unwrap();
        if images.is_empty() {
            return Err("No images in queue".to_string());
        }
        images.iter().cloned().collect::<Vec<String>>()
    };

    // Step 1: Use Gemini 2.0 Flash for content extraction
    let client = Client::default();
    let mut content_parts = vec![ContentPart::from_text(prompt)];

    // Add all images from the queue
    for image_path in image_paths.iter() {
        let mut file = File::open(image_path).map_err(|e| e.to_string())?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).map_err(|e| e.to_string())?;

        let encoded_image = general_purpose::STANDARD.encode(&buffer);
        content_parts.push(ContentPart::from_image_base64("image/png", Arc::from(encoded_image)));
    }

    let chat_req = ChatRequest::new(vec![
        ChatMessage::system("You are an expert content extractor. Extract ALL text, formulas, diagrams, and structured information from the provided images. Be comprehensive and detailed."),
        ChatMessage::user(content_parts),
    ]);

    // Use Gemini 2.0 Flash for extraction
    let extraction_result = client
        .exec_chat("gemini-2.0-flash", chat_req, None)
        .await
        .map_err(|e| e.to_string())?;

    let extracted_content = extraction_result
        .content_text_as_str()
        .unwrap_or("[No extraction]")
        .to_string();

        // Step 2: Send extracted content to advanced AI model via Hugging Face API
    let hf_token = std::env::var("HUGGINGFACE_TOKEN").ok();
    
    if let Some(token) = hf_token {
        let http_client = HttpClient::new();
        
        // Prepare the final prompt for advanced AI processing
        let final_prompt = format!(
            "Based on the extracted content below, provide comprehensive answers:\n\n{}\n\nFor MCQ questions: Identify all possibilities for single correct and multiple correct answers.\nFor coding questions: Provide complete code solutions in the requested language with proper formatting.",
            extracted_content
        );

        // Use a more reliable model endpoint
        let model_endpoint = "https://api-inference.huggingface.co/models/microsoft/DialoGPT-large";
        
        let gpt_response = match http_client
            .post(model_endpoint)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(120)) // 2 minute timeout
            .json(&json!({
                "inputs": final_prompt,
                "parameters": {
                    "max_new_tokens": 2048,
                    "temperature": 0.7,
                    "return_full_text": false,
                    "do_sample": true,
                    "top_p": 0.9
                }
            }))
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<serde_json::Value>().await {
                        Ok(json) => {
                            // Handle Hugging Face API response format
                            if let Some(choices) = json.as_array() {
                                if let Some(first_choice) = choices.first() {
                                    if let Some(text) = first_choice["generated_text"].as_str() {
                                        text.to_string()
                                    } else {
                                        "No generated text in response".to_string()
                                    }
                                } else {
                                    "Empty response from AI model".to_string()
                                }
                            } else if let Some(text) = json["generated_text"].as_str() {
                                text.to_string()
                            } else {
                                "Unexpected response format from AI model".to_string()
                            }
                        }
                        Err(e) => format!("Error parsing AI model response: {}", e)
                    }
                } else {
                    let status = response.status();
                    let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                    
                    // Handle specific error cases
                    if status == 503 {
                        format!(
                            "## BEAST MODE EXTRACTION COMPLETE! 🚀\n\n**Extracted Content:**\n{}\n\n**Note:** Advanced AI processing is temporarily unavailable (Service Unavailable). The extracted content above contains all the information from your images. You can use this content directly or try again later.",
                            extracted_content
                        )
                    } else if status == 400 {
                        format!(
                            "## BEAST MODE EXTRACTION COMPLETE! 🚀\n\n**Extracted Content:**\n{}\n\n**Note:** Advanced AI processing request format error (Bad Request). The extracted content above contains all the information from your images. You can use this content directly.",
                            extracted_content
                        )
                    } else {
                        format!("AI model API error ({}): {}", status, error_text)
                    }
                }
            }
            Err(e) => {
                // Handle network errors gracefully
                format!(
                    "## BEAST MODE EXTRACTION COMPLETE! 🚀\n\n**Extracted Content:**\n{}\n\n**Note:** Network error occurred while connecting to advanced AI processing: {}. The extracted content above contains all the information from your images. You can use this content directly or check your internet connection and try again.",
                    extracted_content, e
                )
            }
        };
        
        Ok(gpt_response)
    } else {
        // Fallback: Return the extracted content with a note
        Ok(format!(
            "## BEAST MODE EXTRACTION COMPLETE! 🚀\n\n**Extracted Content:**\n{}\n\n**Note:** Hugging Face token not configured. The extracted content above contains all the information from your images. Set a Hugging Face token in the app to enable advanced AI processing.",
            extracted_content
        ))
    }
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            move_window,
            nudge_window,
            toggle_window_visibility,
            resize_window,
            call_gemini,
            capture_area,
            capture_full_screen,
            call_gemini_with_image,
            quit_app,
            add_image_to_queue,
            get_queue_length,
            clear_queue,
            call_gemini_with_image_queue,
            set_gemini_api_key,
            get_gemini_api_key,
            set_model,
            call_beast_mode,
            set_hf_token,
            get_hf_token,
        ])
        .setup(|app| {
            // Initialize and manage app-level toggle state
            app.manage(ToggleState { 
                visible: AtomicBool::new(true),
                last_toggle: Mutex::new(Instant::now() - Duration::from_secs(1)),
                last_nudge: Mutex::new(Instant::now() - Duration::from_secs(1)),
            });
            // Initialize image queue
            app.manage(ImageQueue {
                images: Mutex::new(VecDeque::new()),
                last_capture: Mutex::new(Instant::now() - Duration::from_secs(1)),
            });
            // Initialize runtime configuration
            let initial_model = std::env::var("GEMINI_MODEL").unwrap_or_else(|_| "gemini-2.5-pro".to_string());
            let initial_key = std::env::var("GEMINI_API_KEY").ok();
            let initial_hf_token = std::env::var("HUGGINGFACE_TOKEN").ok();
            app.manage(AppConfig {
                api_key: Mutex::new(initial_key),
                model: Mutex::new(initial_model),
                hf_token: Mutex::new(initial_hf_token),
            });
            let window = app.get_webview_window("main").unwrap();
            window.set_always_on_top(true)?;
            window.set_decorations(false)?;
            window.set_content_protected(true)?;
            window.set_skip_taskbar(true)?;
            // window.set_ignore_cursor_events(true)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri app");
}
