use rdev::{listen, Event, EventType, Key};
use std::sync::{mpsc, Mutex};
use std::thread;

static CAPTURE: Mutex<Option<mpsc::Sender<Result<String, String>>>> = Mutex::new(None);

pub fn key_from_name(name: &str) -> Option<Key> {
    match name {
        "RightCtrl" => Some(Key::ControlRight),
        "LeftCtrl" => Some(Key::ControlLeft),
        "RightAlt" => Some(Key::AltGr),
        "LeftAlt" => Some(Key::Alt),
        "CapsLock" => Some(Key::CapsLock),
        "RightShift" => Some(Key::ShiftRight),
        "LeftShift" => Some(Key::ShiftLeft),
        "F1" => Some(Key::F1), "F2" => Some(Key::F2),
        "F3" => Some(Key::F3), "F4" => Some(Key::F4),
        "F5" => Some(Key::F5), "F6" => Some(Key::F6),
        "F7" => Some(Key::F7), "F8" => Some(Key::F8),
        "F9" => Some(Key::F9), "F10" => Some(Key::F10),
        "F11" => Some(Key::F11), "F12" => Some(Key::F12),
        _ => None,
    }
}

fn key_to_name(key: Key) -> Option<String> {
    ["RightCtrl", "LeftCtrl", "RightAlt", "LeftAlt", "CapsLock", "RightShift",
     "LeftShift", "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12"]
        .into_iter().find(|name| key_from_name(name) == Some(key)).map(str::to_owned)
}

/// Exactly one OS hook for the lifetime of the application. Capture requests
/// share this hook; repeated assignment never installs additional listeners.
pub fn spawn_listener<F1, F2, G>(get_target_key: G, on_press: F1, on_release: F2)
where
    F1: Fn() + Send + 'static,
    F2: Fn() + Send + 'static,
    G: Fn() -> Option<Key> + Send + 'static,
{
    thread::spawn(move || {
        let mut held = None;
        let mut captured = None;
        let callback = move |event: Event| {
            match event.event_type {
                EventType::KeyRelease(k) => {
                    if captured == Some(k) { captured = None; }
                    // Match the key that started recording, even if settings changed.
                    if held == Some(k) { held = None; on_release(); }
                }
                EventType::KeyPress(k) => {
                    if captured == Some(k) { return; }
                    let mut capture = CAPTURE.lock().unwrap();
                    if let Some(tx) = capture.as_ref() {
                        let result = if k == Key::Escape {
                            Some(Err("Назначение отменено".into()))
                        } else { key_to_name(k).map(Ok) };
                        if let Some(result) = result {
                            let _ = tx.send(result);
                            *capture = None;
                            captured = Some(k);
                        }
                        return;
                    }
                    drop(capture);
                    if held.is_none() && get_target_key() == Some(k) {
                        held = Some(k);
                        on_press();
                    }
                }
                _ => {}
            }
        };
        if let Err(e) = listen(callback) {
            eprintln!("[hotkey] ошибка глобального слушателя: {e:?}");
        }
    });
}

pub fn capture_single_press(timeout_ms: u64) -> Result<String, String> {
    let (tx, rx) = mpsc::channel();
    {
        let mut slot = CAPTURE.lock().unwrap();
        if slot.is_some() { return Err("Назначение уже выполняется".into()); }
        *slot = Some(tx);
    }
    let result = rx.recv_timeout(std::time::Duration::from_millis(timeout_ms))
        .map_err(|_| "Клавиша не получена. Используйте список клавиш или повторите назначение.".to_string());
    *CAPTURE.lock().unwrap() = None;
    result?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_captured_key_can_be_registered() {
        for key in [Key::ControlRight, Key::AltGr, Key::ShiftLeft, Key::F1, Key::F12] {
            assert_eq!(key_from_name(&key_to_name(key).unwrap()), Some(key));
        }
        assert!(key_to_name(Key::KeyA).is_none());
        assert!(key_from_name("Unknown").is_none());
    }
}
