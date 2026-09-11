use regex::{Regex, NoExpand};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Голосовые команды для RU/UK/EN — распознаются после транскрипции,
/// до финальной пунктуации, и заменяются на управляющие символы/действия.
pub enum VoiceAction {
    NewLine,
    NewParagraph,
    DeleteLastSentence,
}

pub struct ProcessedText {
    pub text: String,
    pub actions: Vec<VoiceAction>,
}

fn voice_command_map() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        // русский
        ("новая строка", "\n"),
        ("новый абзац", "\n\n"),
        ("точка", "."),
        ("запятая", ","),
        ("вопросительный знак", "?"),
        ("восклицательный знак", "!"),
        // украинский
        ("новий рядок", "\n"),
        ("новий абзац", "\n\n"),
        ("крапка", "."),
        ("кома", ","),
        // английский
        ("new line", "\n"),
        ("new paragraph", "\n\n"),
        ("period", "."),
        ("comma", ","),
        ("question mark", "?"),
        ("exclamation mark", "!"),
    ])
}

/// Возвращает true, если фраза — команда "удалить последнее предложение" на любом из языков.
pub fn is_delete_last_sentence_command(raw: &str) -> bool {
    let lower = raw.trim().to_lowercase();
    matches!(
        lower.as_str(),
        "удали последнее предложение"
            | "удалить последнее предложение"
            | "видали останнє речення"
            | "delete last sentence"
    )
}

/// Заменяет проговорённые команды пунктуации на символы.
pub fn apply_voice_commands(raw: &str, enabled: bool) -> String {
    if !enabled {
        return raw.to_string();
    }
    static COMMANDS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    let commands = COMMANDS.get_or_init(|| {
        let map = voice_command_map();
        let mut phrases: Vec<_> = map.into_iter().collect();
        phrases.sort_by_key(|(phrase, _)| std::cmp::Reverse(phrase.len()));
        phrases.into_iter().map(|(phrase, replacement)| {
            (Regex::new(&format!(r"(?i)\b{}\b", regex::escape(phrase))).unwrap(), replacement)
        }).collect()
    });
    let mut result = raw.to_string();
    // Сортируем по длине фразы по убыванию, чтобы "новый абзац" не резалось "новая строка"
    for (re, replacement) in commands {
        result = re.replace_all(&result, NoExpand(replacement)).to_string();
    }
    result
}

/// Лёгкая офлайн-пунктуация и капитализация — работает как страховка сверху
/// вывода Whisper (крупные модели сами расставляют знаки, малые — хуже).
pub fn auto_punctuate(raw: &str, enabled: bool) -> String {
    if !enabled || raw.trim().is_empty() {
        return raw.trim().to_string();
    }

    let mut text = raw.trim().to_string();

    // Убираем повторные пробелы
    static SPACES: OnceLock<Regex> = OnceLock::new();
    let re_spaces = SPACES.get_or_init(|| Regex::new(r"[^\S\r\n]+").unwrap());
    text = re_spaces.replace_all(&text, " ").to_string();

    // Пробел перед знаком пунктуации — убираем
    static PUNCT: OnceLock<Regex> = OnceLock::new();
    let re_space_before_punct = PUNCT.get_or_init(|| Regex::new(r"[^\S\r\n]+([.,!?;:])").unwrap());
    text = re_space_before_punct.replace_all(&text, "$1").to_string();

    // Капитализация первой буквы предложения (после . ! ? и в начале строки)
    let mut chars: Vec<char> = text.chars().collect();
    let mut capitalize_next = true;
    for i in 0..chars.len() {
        if capitalize_next && chars[i].is_alphabetic() {
            chars[i] = chars[i].to_uppercase().next().unwrap_or(chars[i]);
            capitalize_next = false;
        }
        if matches!(chars[i], '.' | '!' | '?' | '\n') {
            capitalize_next = true;
        }
    }
    text = chars.into_iter().collect();

    // Точка в конце, если предложение не завершено другим знаком
    if !text.is_empty() && !matches!(text.chars().last().unwrap(), '.' | '!' | '?' | ':' | ',') {
        text.push('.');
    }

    text
}

/// Применяет пользовательский словарь: точечные замены/исправления написания имён,
/// терминов и аббревиатур, которые модель обычно неверно расшифровывает.
pub fn apply_custom_dictionary(text: &str, dictionary: &[String]) -> String {
    let mut result = text.to_string();
    for term in dictionary {
        // Простое сопоставление без учёта регистра: если слово похоже фонетически,
        // в реальном проде здесь стоит fuzzy-matching (например, crate `strsim`).
        let re = Regex::new(&format!(r"(?i)\b{}\b", regex::escape(term)));
        if let Ok(re) = re {
            result = re.replace_all(&result, NoExpand(term)).to_string();
        }
    }
    result
}

/// Облачная грамматическая коррекция через LanguageTool API (опционально).
/// Используется только если пользователь включил cloud_enabled и указал ключ/эндпоинт.
pub async fn cloud_grammar_correct(
    text: &str,
    api_key: &str,
    language_hint: &str,
) -> anyhow::Result<String> {
    if api_key.trim().is_empty() {
        return Ok(text.to_string());
    }

    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).build()?;
    let lang = match language_hint {
        "ru" => "ru-RU",
        "uk" => "uk-UA",
        "en" => "en-US",
        _ => "auto",
    };

    #[derive(serde::Serialize)]
    struct Req<'a> {
        model: &'a str,
        messages: Vec<Msg<'a>>,
    }
    #[derive(serde::Serialize)]
    struct Msg<'a> {
        role: &'a str,
        content: String,
    }
    #[derive(serde::Deserialize)]
    struct Resp {
        choices: Vec<Choice>,
    }
    #[derive(serde::Deserialize)]
    struct Choice {
        message: ChoiceMsg,
    }
    #[derive(serde::Deserialize)]
    struct ChoiceMsg {
        content: String,
    }

    let prompt = format!(
        "Исправь грамматику, пунктуацию и опечатки в следующем тексте (язык: {lang}). \
         Верни ТОЛЬКО исправленный текст без пояснений:\n\n{text}"
    );

    let resp = client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_key)
        .json(&Req {
            model: "gpt-4o-mini",
            messages: vec![Msg { role: "user", content: prompt }],
        })
        .send()
        .await?
        .error_for_status()?
        .json::<Resp>()
        .await?;

    Ok(resp
        .choices
        .first()
        .map(|c| c.message.content.trim().to_string())
        .unwrap_or_else(|| text.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn punctuation_preserves_voice_command_line_breaks() {
        let text = apply_voice_commands("привет новая строка мир новый абзац дальше", true);
        let text = auto_punctuate(&text, true);
        assert!(text.contains('\n'));
        assert!(text.contains("\n\n"));
        assert!(text.ends_with("Дальше."));
    }
    #[test]
    fn dictionary_does_not_interpret_dollar_signs_as_capture_groups() {
        assert_eq!(apply_custom_dictionary("usd$100", &["USD$100".into()]), "USD$100");
    }
    #[test]
    fn disabled_commands_preserve_words() {
        assert_eq!(apply_voice_commands("новая строка", false), "новая строка");
    }
}
