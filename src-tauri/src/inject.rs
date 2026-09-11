use enigo::{Enigo, Keyboard, Settings as EnigoSettings};

pub fn foreground_window() -> Option<isize> {
    #[cfg(windows)]
    {
        let window = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
        if window.0.is_null() { None } else { Some(window.0 as isize) }
    }
    #[cfg(not(windows))]
    { None }
}

/// Вставляет текст в текущее активное окно (то, что было в фокусе
/// на момент отпускания горячей клавиши).
///
/// mode = "type"            -> имитация набора символов, работает абсолютно везде,
///                              включая поля без поддержки вставки, но медленнее на длинном тексте.
/// mode = "clipboard_paste"  -> копирует текст в буфер обмена и отправляет Ctrl+V,
///                              быстрее, но временно перезаписывает буфер обмена пользователя.
pub fn insert_text(text: &str, mode: &str) -> anyhow::Result<()> {
    if text.trim().is_empty() {
        return Ok(());
    }

    match mode {
        "clipboard_paste" => insert_via_clipboard(text),
        _ => insert_via_typing(text),
    }
}

fn insert_via_typing(text: &str) -> anyhow::Result<()> {
    let mut enigo = Enigo::new(&EnigoSettings::default())
        .map_err(|e| anyhow::anyhow!("Не удалось инициализировать эмуляцию ввода: {e:?}"))?;
    enigo
        .text(text)
        .map_err(|e| anyhow::anyhow!("Ошибка ввода текста: {e:?}"))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn insert_via_clipboard(text: &str) -> anyhow::Result<()> {
    use enigo::{Direction, Key};

    // Сохраняем текущий буфер, чтобы вернуть его после вставки — не ломаем пользователю
    // то, что он скопировал ранее.
    let previous = get_clipboard_text();

    set_clipboard_text(text)?;

    let mut enigo = Enigo::new(&EnigoSettings::default())
        .map_err(|e| anyhow::anyhow!("Не удалось инициализировать эмуляцию ввода: {e:?}"))?;
    enigo.key(Key::Control, Direction::Press)?;
    let paste = enigo.key(Key::Unicode('v'), Direction::Click);
    let release = enigo.key(Key::Control, Direction::Release);
    paste?;
    release?;

    // Небольшая задержка перед восстановлением старого буфера, чтобы вставка успела произойти
    std::thread::sleep(std::time::Duration::from_millis(150));
    if get_clipboard_text().as_deref() == Some(text) {
        if let Some(prev) = previous { let _ = set_clipboard_text(&prev); }
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn insert_via_clipboard(text: &str) -> anyhow::Result<()> {
    // На macOS/Linux используется тот же принцип через Cmd+V — реализуется
    // через tauri-plugin-clipboard-manager на стороне фронтенда при необходимости.
    insert_via_typing(text)
}

// ВАЖНО: crate `clipboard-win` объявлен в Cargo.toml только как
// target-зависимость для Windows (`[target.'cfg(windows)'.dependencies]`).
// На macOS/Linux этого крейта физически нет в дереве зависимостей, поэтому
// `use clipboard_win::...` должен находиться СТРОГО внутри cfg(windows)-блока,
// а не на верхнем уровне модуля — иначе получаем E0432 unresolved import.
#[cfg(target_os = "windows")]
fn get_clipboard_text() -> Option<String> {
    use clipboard_win::{formats, get_clipboard};
    get_clipboard(formats::Unicode).ok()
}

#[cfg(not(target_os = "windows"))]
fn get_clipboard_text() -> Option<String> {
    None
}

#[cfg(target_os = "windows")]
fn set_clipboard_text(text: &str) -> anyhow::Result<()> {
    use clipboard_win::{formats, set_clipboard};
    set_clipboard(formats::Unicode, text)
        .map_err(|e| anyhow::anyhow!("Ошибка буфера обмена: {e:?}"))?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn set_clipboard_text(_text: &str) -> anyhow::Result<()> {
    Ok(())
}
