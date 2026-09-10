use rdev::{listen, Event, EventType, Key};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

/// Переводит человекочитаемое имя хоткея (как хранится в конфиге) в rdev::Key.
///
/// ВАЖНО: F13/F14 сюда сознательно не включены — в rdev 0.5.3 (версия,
/// которая реально резолвится по нашим Cargo.toml-ограничениям) этих
/// вариантов enum'а Key не существует (E0599 no variant named `F13`).
/// Расширенные функциональные клавиши появились в более поздних версиях
/// крейта; если понадобятся — обновить rdev и вернуть эти строки.
pub fn key_from_name(name: &str) -> Option<Key> {
    match name {
        "RightCtrl" => Some(Key::ControlRight),
        "LeftCtrl" => Some(Key::ControlLeft),
        "RightAlt" => Some(Key::AltGr),
        "LeftAlt" => Some(Key::Alt),
        "CapsLock" => Some(Key::CapsLock),
        "RightShift" => Some(Key::ShiftRight),
        _ => None,
    }
}

/// Запускает фоновый listener, который вызывает on_press при нажатии
/// заданной клавиши и on_release при её отпускании (режим "зажал — говоришь").
/// target_key читается динамически через closure, чтобы реагировать на
/// изменение хоткея в настройках без перезапуска приложения.
pub fn spawn_listener<F1, F2, G>(get_target_key: G, on_press: F1, on_release: F2)
where
    F1: Fn() + Send + 'static,
    F2: Fn() + Send + 'static,
    G: Fn() -> Option<Key> + Send + 'static,
{
    let is_down = Arc::new(AtomicBool::new(false));

    thread::spawn(move || {
        let is_down = is_down.clone();
        let callback = move |event: Event| {
            let target = match get_target_key() {
                Some(k) => k,
                None => return,
            };
            match event.event_type {
                EventType::KeyPress(k) if k == target => {
                    if !is_down.swap(true, Ordering::SeqCst) {
                        on_press();
                    }
                }
                EventType::KeyRelease(k) if k == target => {
                    if is_down.swap(false, Ordering::SeqCst) {
                        on_release();
                    }
                }
                _ => {}
            }
        };
        if let Err(e) = listen(callback) {
            eprintln!("[hotkey] ошибка глобального слушателя клавиатуры: {e:?}");
        }
    });
}

/// Используется настройками для захвата "следующей нажатой клавиши" —
/// слушает один раз и возвращает читаемое имя.
pub fn capture_single_press(timeout_ms: u64) -> Option<String> {
    use std::sync::mpsc::channel;
    let (tx, rx) = channel();

    thread::spawn(move || {
        let _ = listen(move |event: Event| {
            if let EventType::KeyPress(k) = event.event_type {
                let _ = tx.send(key_to_name(k));
            }
        });
    });

    rx.recv_timeout(std::time::Duration::from_millis(timeout_ms)).ok()
}

fn key_to_name(key: Key) -> String {
    match key {
        Key::ControlRight => "RightCtrl".into(),
        Key::ControlLeft => "LeftCtrl".into(),
        Key::AltGr => "RightAlt".into(),
        Key::Alt => "LeftAlt".into(),
        Key::CapsLock => "CapsLock".into(),
        Key::ShiftRight => "RightShift".into(),
        other => format!("{other:?}"),
    }
}
