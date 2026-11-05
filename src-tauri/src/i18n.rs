//! Strings shown by the Rust side (tray menu, OS notifications). The webview has its own i18n.

use crate::settings::Settings;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Pt,
    En,
}

pub fn lang(settings: &Settings) -> Lang {
    let tag = if settings.ui_language == "auto" {
        sys_locale::get_locale().unwrap_or_default()
    } else {
        settings.ui_language.clone()
    };
    if tag.to_lowercase().starts_with("pt") {
        Lang::Pt
    } else {
        Lang::En
    }
}

pub fn tr(lang: Lang, key: &str) -> &'static str {
    use Lang::*;
    match (key, lang) {
        ("open", Pt) => "Abrir Kaku",
        ("open", En) => "Open Kaku",
        ("palette", Pt) => "Abrir paleta de ações",
        ("palette", En) => "Open action palette",
        ("pause", Pt) => "Pausar atalhos",
        ("pause", En) => "Pause shortcuts",
        ("copyOriginal", Pt) => "Copiar texto original da última ação",
        ("copyOriginal", En) => "Copy original text of last action",
        ("quit", Pt) => "Sair",
        ("quit", En) => "Quit",
        ("noSelection", Pt) => "Nenhum texto selecionado. Selecione um texto e tente de novo.",
        ("noSelection", En) => "No text selected. Select some text and try again.",
        ("errorTitle", Pt) => "Kaku: falha na ação",
        ("errorTitle", En) => "Kaku: action failed",
        ("missingKey", Pt) => "Configure sua API key nas configurações do Kaku.",
        ("missingKey", En) => "Set your API key in Kaku settings.",
        ("updateTitle", Pt) => "Atualização disponível",
        ("updateTitle", En) => "Update available",
        ("updateBody", Pt) => "Abra Kaku › Sobre para instalar a versão",
        ("updateBody", En) => "Open Kaku › About to install version",
        ("clipboardDone", Pt) => "Resultado copiado para a área de transferência.",
        ("clipboardDone", En) => "Result copied to the clipboard.",
        ("shortcutConflict", Pt) => {
            "Alguns atalhos não puderam ser registrados (já em uso?). Veja Configurações › Atalhos."
        }
        ("shortcutConflict", En) => {
            "Some shortcuts could not be registered (already in use?). See Settings › Shortcuts."
        }
        _ => "",
    }
}
