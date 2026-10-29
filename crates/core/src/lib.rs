//! Cœur de l'appli « Documents » (visionneuse de PDF et d'images pour Windows).
//!
//! Toute la logique vit ici, sans dépendance à Tauri, pour être testée avec `cargo test`.

pub mod annotate;
mod document;
mod exif;
pub mod folder;
pub mod form;
pub mod reduce;
pub mod signature;
mod kind;
mod library;
mod pdf;
mod picture;
mod rearrange;

pub use document::{DocDetails, DocId, DocInfo, NormRect, PageRef, PageSize, PageText, RenderedImage, Rotation, SearchHit};
pub use kind::DocumentKind;
pub use library::Library;
pub use picture::{CropRect, Retouch};

/// Ce qui peut mal se passer, avec un message lisible par l'utilisateur.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Impossible de lire le fichier : {0}")]
    Io(#[from] std::io::Error),
    #[error("Ce type de fichier n'est pas pris en charge.")]
    UnsupportedFormat,
    #[error("Les photos HEIC ne sont pas encore prises en charge.")]
    HeicNotYetSupported,
    #[error("Le moteur PDF n'a pas pu démarrer : {0}")]
    PdfEngineUnavailable(String),
    #[error("Ce PDF est protégé par un mot de passe.")]
    PasswordProtected,
    #[error("Ce PDF est illisible ou abîmé : {0}")]
    BrokenPdf(String),
    #[error("Cette image est illisible ou abîmée : {0}")]
    BrokenImage(String),
    #[error("Ce document n'est plus ouvert.")]
    UnknownDocument,
    #[error("Cette page n'existe pas.")]
    UnknownPage,
    #[error("Il n'y a aucune page à enregistrer.")]
    NothingToSave,
    #[error("Impossible d'enregistrer : {0}")]
    SaveFailed(String),
}

pub type Result<T> = std::result::Result<T, Error>;
