//! Langue de l'appli : celle de l'affichage de Windows, sinon l'anglais.
//!
//! L'interface a ses propres textes (ui/src/locales/) ; ici, seulement ceux que le cœur Rust
//! montre lui-même : messages d'erreur, icône près de l'horloge, types de fichiers.

use std::sync::OnceLock;

use documents_core::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    Fr,
    Es,
    It,
    Pt,
    Zh,
    Ja,
}

impl Lang {
    /// La langue d'une étiquette comme « fr-FR », « zh-Hans-CN » ou « pt_BR.UTF-8 ».
    pub fn from_tag(tag: &str) -> Option<Lang> {
        let base = tag.split(['-', '_', '.']).next()?.to_ascii_lowercase();
        Some(match base.as_str() {
            "en" => Lang::En,
            "fr" => Lang::Fr,
            "es" => Lang::Es,
            "it" => Lang::It,
            "pt" => Lang::Pt,
            "zh" => Lang::Zh,
            "ja" => Lang::Ja,
            _ => return None,
        })
    }

    /// La langue d'un identifiant de langue de Windows (LANGID) : sa partie basse (LANG_*).
    #[cfg(any(windows, test))]
    pub fn from_langid(id: u16) -> Option<Lang> {
        Some(match id & 0x3ff {
            0x09 => Lang::En,
            0x0c => Lang::Fr,
            0x0a => Lang::Es,
            0x10 => Lang::It,
            0x16 => Lang::Pt,
            0x04 => Lang::Zh,
            0x11 => Lang::Ja,
            _ => return None,
        })
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Fr => "fr",
            Lang::Es => "es",
            Lang::It => "it",
            Lang::Pt => "pt",
            Lang::Zh => "zh",
            Lang::Ja => "ja",
        }
    }

    /// Le texte de cette langue parmi [en, fr, es, it, pt, zh, ja].
    fn pick(self, texts: [&'static str; 7]) -> &'static str {
        texts[self as usize]
    }
}

/// La langue choisie au lancement. `DOCUMENTS_LANG=fr` la force (essais).
pub fn lang() -> Lang {
    static LANG: OnceLock<Lang> = OnceLock::new();
    *LANG.get_or_init(|| {
        std::env::var("DOCUMENTS_LANG")
            .ok()
            .and_then(|v| Lang::from_tag(&v))
            .or_else(system_lang)
            .unwrap_or(Lang::En)
    })
}

/// La langue d'affichage choisie dans les paramètres de Windows.
#[cfg(windows)]
fn system_lang() -> Option<Lang> {
    Lang::from_langid(unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() })
}

/// Hors Windows (développement) : la langue de la session.
#[cfg(not(windows))]
fn system_lang() -> Option<Lang> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.is_empty())
        .and_then(|v| Lang::from_tag(&v))
}

/// Donne la langue à la page avant son premier affichage (lue par ui/src/i18n.ts).
pub fn script() -> String {
    format!("window.__DOCUMENTS_LANGUE__ = {:?};", lang().code())
}

/// Les textes montrés par le cœur Rust (icône près de l'horloge, déclaration à Windows).
#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug)]
pub enum Text {
    QuickInfo,
    QuickOff,
    Quit,
    QuickTooltip,
    AppDescription,
    KindPdf,
    KindJpeg,
    KindPng,
    KindWebp,
    KindGif,
    KindHeic,
    KindTiff,
    KindBmp,
    KindRaw,
    KindAi,
    KindJp2,
    KindHdr,
    KindExr,
    KindIco,
    KindPnm,
    KindImage,
    SettingsFailed,
}

#[cfg(any(windows, test))]
pub fn tr(text: Text) -> &'static str {
    tr_in(lang(), text)
}

#[cfg(any(windows, test))]
fn tr_in(l: Lang, text: Text) -> &'static str {
    l.pick(match text {
        Text::QuickInfo => [
            "Quick preview: Space in File Explorer",
            "Aperçu rapide : Espace dans l'Explorateur",
            "Vista rápida: Espacio en el Explorador de archivos",
            "Anteprima rapida: Spazio in Esplora file",
            "Pré-visualização rápida: Espaço no Explorador de Ficheiros",
            "快速预览：在文件资源管理器中按空格键",
            "クイックプレビュー：エクスプローラーでスペースキー",
        ],
        Text::QuickOff => [
            "Turn off quick preview",
            "Désactiver l'aperçu rapide",
            "Desactivar la vista rápida",
            "Disattiva l'anteprima rapida",
            "Desativar a pré-visualização rápida",
            "关闭快速预览",
            "クイックプレビューをオフにする",
        ],
        Text::Quit => ["Quit", "Quitter", "Salir", "Esci", "Sair", "退出", "終了"],
        Text::QuickTooltip => [
            "Documents: quick preview",
            "Documents : aperçu rapide",
            "Documents: vista rápida",
            "Documents: anteprima rapida",
            "Documents: pré-visualização rápida",
            "Documents：快速预览",
            "Documents：クイックプレビュー",
        ],
        Text::AppDescription => [
            "PDF and image viewer",
            "Visionneuse de PDF et d'images",
            "Visor de PDF e imágenes",
            "Visualizzatore di PDF e immagini",
            "Visualizador de PDF e imagens",
            "PDF 和图像查看器",
            "PDF と画像のビューアー",
        ],
        Text::KindPdf => ["PDF document", "Document PDF", "Documento PDF", "Documento PDF", "Documento PDF", "PDF 文档", "PDF 書類"],
        Text::KindJpeg => ["JPEG image", "Image JPEG", "Imagen JPEG", "Immagine JPEG", "Imagem JPEG", "JPEG 图像", "JPEG 画像"],
        Text::KindPng => ["PNG image", "Image PNG", "Imagen PNG", "Immagine PNG", "Imagem PNG", "PNG 图像", "PNG 画像"],
        Text::KindWebp => ["WebP image", "Image WebP", "Imagen WebP", "Immagine WebP", "Imagem WebP", "WebP 图像", "WebP 画像"],
        Text::KindGif => ["GIF image", "Image GIF", "Imagen GIF", "Immagine GIF", "Imagem GIF", "GIF 图像", "GIF 画像"],
        Text::KindHeic => ["HEIC photo", "Photo HEIC", "Foto HEIC", "Foto HEIC", "Fotografia HEIC", "HEIC 照片", "HEIC 写真"],
        Text::KindTiff => ["TIFF image", "Image TIFF", "Imagen TIFF", "Immagine TIFF", "Imagem TIFF", "TIFF 图像", "TIFF 画像"],
        Text::KindBmp => ["BMP image", "Image BMP", "Imagen BMP", "Immagine BMP", "Imagem BMP", "BMP 图像", "BMP 画像"],
        Text::KindRaw => ["RAW photo", "Photo RAW", "Foto RAW", "Foto RAW", "Fotografia RAW", "RAW 照片", "RAW 写真"],
        Text::KindAi => [
            "Vector drawing (AI)",
            "Dessin vectoriel (AI)",
            "Dibujo vectorial (AI)",
            "Disegno vettoriale (AI)",
            "Desenho vetorial (AI)",
            "矢量图 (AI)",
            "ベクター画像 (AI)",
        ],
        Text::KindJp2 => ["JPEG 2000 image", "Image JPEG 2000", "Imagen JPEG 2000", "Immagine JPEG 2000", "Imagem JPEG 2000", "JPEG 2000 图像", "JPEG 2000 画像"],
        Text::KindHdr => ["HDR image", "Image HDR", "Imagen HDR", "Immagine HDR", "Imagem HDR", "HDR 图像", "HDR 画像"],
        Text::KindExr => ["OpenEXR image", "Image OpenEXR", "Imagen OpenEXR", "Immagine OpenEXR", "Imagem OpenEXR", "OpenEXR 图像", "OpenEXR 画像"],
        Text::KindIco => ["Icon", "Icône", "Icono", "Icona", "Ícone", "图标", "アイコン"],
        Text::KindPnm => ["PPM image", "Image PPM", "Imagen PPM", "Immagine PPM", "Imagem PPM", "PPM 图像", "PPM 画像"],
        Text::KindImage => ["Image", "Image", "Imagen", "Immagine", "Imagem", "图像", "画像"],
        Text::SettingsFailed => [
            "Couldn't open Windows Settings",
            "Impossible d'ouvrir les réglages de Windows",
            "No se pudo abrir la Configuración de Windows",
            "Impossibile aprire le Impostazioni di Windows",
            "Não foi possível abrir as Definições do Windows",
            "无法打开 Windows 设置",
            "Windows の設定を開けませんでした",
        ],
    })
}

/// Le message d'une erreur du cœur, dans la langue de l'appli. Le détail technique (entre
/// parenthèses dans le texte anglais du cœur) reste tel quel.
pub fn error(e: &Error) -> String {
    error_in(lang(), e)
}

fn error_in(l: Lang, e: &Error) -> String {
    let (text, detail) = match e {
        Error::Io(io) => (
            [
                "Couldn't read the file: {0}",
                "Impossible de lire le fichier : {0}",
                "No se pudo leer el archivo: {0}",
                "Impossibile leggere il file: {0}",
                "Não foi possível ler o ficheiro: {0}",
                "无法读取文件：{0}",
                "ファイルを読み込めませんでした：{0}",
            ],
            io.to_string(),
        ),
        Error::UnsupportedFormat => (
            [
                "This type of file isn't supported.",
                "Ce type de fichier n'est pas pris en charge.",
                "Este tipo de archivo no es compatible.",
                "Questo tipo di file non è supportato.",
                "Este tipo de ficheiro não é suportado.",
                "不支持此类型的文件。",
                "この種類のファイルには対応していません。",
            ],
            String::new(),
        ),
        Error::HeicUnavailable(d) => (
            [
                "HEIC photos can't be opened: the HEIC decoder is missing ({0}).",
                "Impossible d'ouvrir les photos HEIC : le décodeur HEIC est absent ({0}).",
                "No se pueden abrir las fotos HEIC: falta el decodificador HEIC ({0}).",
                "Impossibile aprire le foto HEIC: manca il decodificatore HEIC ({0}).",
                "Não é possível abrir fotografias HEIC: falta o descodificador HEIC ({0}).",
                "无法打开 HEIC 照片：缺少 HEIC 解码器（{0}）。",
                "HEIC 写真を開けません：HEIC デコーダーがありません（{0}）。",
            ],
            d.clone(),
        ),
        Error::RawWithoutPreview => (
            [
                "This RAW photo has no preview that can be shown.",
                "Cette photo RAW ne contient pas d'aperçu affichable.",
                "Esta foto RAW no contiene ninguna vista previa que se pueda mostrar.",
                "Questa foto RAW non contiene un'anteprima visualizzabile.",
                "Esta fotografia RAW não contém nenhuma pré-visualização que possa ser mostrada.",
                "此 RAW 照片没有可显示的预览图。",
                "この RAW 写真には表示できるプレビューがありません。",
            ],
            String::new(),
        ),
        Error::PdfEngineUnavailable(d) => (
            [
                "The PDF engine couldn't start: {0}",
                "Le moteur PDF n'a pas pu démarrer : {0}",
                "No se pudo iniciar el motor PDF: {0}",
                "Impossibile avviare il motore PDF: {0}",
                "Não foi possível iniciar o motor de PDF: {0}",
                "PDF 引擎无法启动：{0}",
                "PDF エンジンを起動できませんでした：{0}",
            ],
            d.clone(),
        ),
        Error::PasswordProtected => (
            [
                "This PDF is password-protected.",
                "Ce PDF est protégé par un mot de passe.",
                "Este PDF está protegido con contraseña.",
                "Questo PDF è protetto da password.",
                "Este PDF está protegido por palavra-passe.",
                "此 PDF 受密码保护。",
                "この PDF はパスワードで保護されています。",
            ],
            String::new(),
        ),
        Error::BrokenPdf(d) => (
            [
                "This PDF is unreadable or damaged: {0}",
                "Ce PDF est illisible ou abîmé : {0}",
                "Este PDF es ilegible o está dañado: {0}",
                "Questo PDF è illeggibile o danneggiato: {0}",
                "Este PDF está ilegível ou danificado: {0}",
                "此 PDF 无法读取或已损坏：{0}",
                "この PDF は読み込めないか、壊れています：{0}",
            ],
            d.clone(),
        ),
        Error::BrokenImage(d) => (
            [
                "This image is unreadable or damaged: {0}",
                "Cette image est illisible ou abîmée : {0}",
                "Esta imagen es ilegible o está dañada: {0}",
                "Questa immagine è illeggibile o danneggiata: {0}",
                "Esta imagem está ilegível ou danificada: {0}",
                "此图像无法读取或已损坏：{0}",
                "この画像は読み込めないか、壊れています：{0}",
            ],
            d.clone(),
        ),
        Error::NoSignatureFound => (
            [
                "No signature was found in this image.",
                "Aucune signature n'a été trouvée sur cette image.",
                "No se ha encontrado ninguna firma en esta imagen.",
                "Nessuna firma trovata in questa immagine.",
                "Não foi encontrada nenhuma assinatura nesta imagem.",
                "在此图像中未找到签名。",
                "この画像に署名が見つかりませんでした。",
            ],
            String::new(),
        ),
        Error::UnknownDocument => (
            [
                "This document is no longer open.",
                "Ce document n'est plus ouvert.",
                "Este documento ya no está abierto.",
                "Questo documento non è più aperto.",
                "Este documento já não está aberto.",
                "此文档已不再打开。",
                "この書類はもう開かれていません。",
            ],
            String::new(),
        ),
        Error::UnknownPage => (
            [
                "This page doesn't exist.",
                "Cette page n'existe pas.",
                "Esta página no existe.",
                "Questa pagina non esiste.",
                "Esta página não existe.",
                "此页面不存在。",
                "このページは存在しません。",
            ],
            String::new(),
        ),
        Error::NothingToSave => (
            [
                "There are no pages to save.",
                "Il n'y a aucune page à enregistrer.",
                "No hay ninguna página que guardar.",
                "Non ci sono pagine da salvare.",
                "Não há páginas para guardar.",
                "没有可保存的页面。",
                "保存するページがありません。",
            ],
            String::new(),
        ),
        Error::SaveFailed(d) => (
            [
                "Couldn't save: {0}",
                "Impossible d'enregistrer : {0}",
                "No se pudo guardar: {0}",
                "Impossibile salvare: {0}",
                "Não foi possível guardar: {0}",
                "无法保存：{0}",
                "保存できませんでした：{0}",
            ],
            d.clone(),
        ),
    };
    l.pick(text).replace("{0}", &detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnait_les_etiquettes_de_langue() {
        assert_eq!(Lang::from_tag("fr-FR"), Some(Lang::Fr));
        assert_eq!(Lang::from_tag("zh-Hans-CN"), Some(Lang::Zh));
        assert_eq!(Lang::from_tag("pt_BR.UTF-8"), Some(Lang::Pt));
        assert_eq!(Lang::from_tag("JA"), Some(Lang::Ja));
        assert_eq!(Lang::from_tag("de-DE"), None);
        assert_eq!(Lang::from_tag(""), None);
    }

    #[test]
    fn reconnait_les_langues_de_windows() {
        assert_eq!(Lang::from_langid(0x040c), Some(Lang::Fr)); // fr-FR
        assert_eq!(Lang::from_langid(0x080c), Some(Lang::Fr)); // fr-BE
        assert_eq!(Lang::from_langid(0x0409), Some(Lang::En)); // en-US
        assert_eq!(Lang::from_langid(0x0804), Some(Lang::Zh)); // zh-CN
        assert_eq!(Lang::from_langid(0x0416), Some(Lang::Pt)); // pt-BR
        assert_eq!(Lang::from_langid(0x0411), Some(Lang::Ja));
        assert_eq!(Lang::from_langid(0x0407), None); // de-DE
    }

    #[test]
    fn les_codes_vont_avec_les_langues() {
        for l in [Lang::En, Lang::Fr, Lang::Es, Lang::It, Lang::Pt, Lang::Zh, Lang::Ja] {
            assert_eq!(Lang::from_tag(l.code()), Some(l));
        }
    }

    #[test]
    fn traduit_les_erreurs_avec_leur_detail() {
        let e = Error::SaveFailed("disk full".into());
        assert_eq!(error_in(Lang::Fr, &e), "Impossible d'enregistrer : disk full");
        assert_eq!(error_in(Lang::En, &e), "Couldn't save: disk full");
        assert_eq!(error_in(Lang::En, &Error::PasswordProtected), Error::PasswordProtected.to_string());
        assert_eq!(error_in(Lang::Ja, &Error::UnknownPage), "このページは存在しません。");
    }

    #[test]
    fn chaque_texte_existe_dans_chaque_langue() {
        for l in [Lang::En, Lang::Fr, Lang::Es, Lang::It, Lang::Pt, Lang::Zh, Lang::Ja] {
            for t in [
                Text::QuickInfo, Text::QuickOff, Text::Quit, Text::QuickTooltip, Text::AppDescription, Text::KindPdf, Text::KindJpeg,
                Text::KindPng, Text::KindWebp, Text::KindGif, Text::KindHeic, Text::KindTiff, Text::KindBmp, Text::KindRaw,
                Text::KindAi, Text::KindJp2, Text::KindHdr, Text::KindExr, Text::KindIco, Text::KindPnm, Text::KindImage,
                Text::SettingsFailed,
            ] {
                assert!(!tr_in(l, t).is_empty());
            }
        }
        assert_eq!(tr(Text::Quit), tr_in(lang(), Text::Quit));
    }

    #[test]
    fn le_script_donne_un_code_de_langue() {
        let s = script();
        assert!(s.starts_with("window.__DOCUMENTS_LANGUE__ = \""), "{s}");
    }
}
