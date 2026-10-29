//! Cœur de l'appli « Documents » (visionneuse de PDF et d'images pour Windows).
//!
//! Toute la logique vit ici, sans dépendance à Tauri, pour être testée avec `cargo test`.

pub mod annotate;
mod document;
mod exif;
pub mod folder;
pub mod form;
mod heif;
mod import;
pub mod reduce;
pub mod signature;
mod kind;
mod library;
mod pdf;
mod picture;
mod raw;
mod rearrange;
mod tiffdir;

pub use document::{DocDetails, DocId, DocInfo, NormRect, PageRef, PageSize, PageText, RenderedImage, Rotation, SearchHit};
pub use heif::available as heic_available;
pub use kind::{is_openable_name, is_picture_name, DocumentKind, EXTENSIONS};
pub use library::Library;
pub use picture::{CropRect, Retouch};

/// Ce qui peut mal se passer, avec un message lisible par l'utilisateur (en anglais ; l'appli
/// le traduit, voir src-tauri/src/i18n.rs).
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Couldn't read the file: {0}")]
    Io(#[from] std::io::Error),
    #[error("This type of file isn't supported.")]
    UnsupportedFormat,
    #[error("HEIC photos can't be opened: the HEIC decoder is missing ({0}).")]
    HeicUnavailable(String),
    #[error("This RAW photo has no preview that can be shown.")]
    RawWithoutPreview,
    #[error("The PDF engine couldn't start: {0}")]
    PdfEngineUnavailable(String),
    #[error("This PDF is password-protected.")]
    PasswordProtected,
    #[error("This PDF is unreadable or damaged: {0}")]
    BrokenPdf(String),
    #[error("This image is unreadable or damaged: {0}")]
    BrokenImage(String),
    #[error("No signature was found in this image.")]
    NoSignatureFound,
    #[error("This document is no longer open.")]
    UnknownDocument,
    #[error("This page doesn't exist.")]
    UnknownPage,
    #[error("There are no pages to save.")]
    NothingToSave,
    #[error("Couldn't save: {0}")]
    SaveFailed(String),
}

pub type Result<T> = std::result::Result<T, Error>;
