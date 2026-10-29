use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::annotate::Annotation;
use crate::DocumentKind;

/// Identifiant d'un document ouvert (unique pendant la vie de l'appli).
pub type DocId = u32;

/// Taille d'affichage d'une page « à 100 % », en pixels d'écran (1 point PDF = 4/3 px).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
}

/// Ce que l'interface a besoin de savoir sur un document qui vient d'être ouvert.
#[derive(Debug, Clone, Serialize)]
pub struct DocInfo {
    pub id: DocId,
    pub name: String,
    pub path: PathBuf,
    pub kind: DocumentKind,
    pub pages: Vec<PageSize>,
    /// Photo retouchée (recadrée, couleurs…) : son fichier d'origine ne correspond plus.
    pub edited: bool,
}

/// Ce que montre l'inspecteur (Ctrl+I) : le fichier, et pour un PDF ses informations.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct DocDetails {
    /// Taille du fichier, en octets.
    pub file_size: u64,
    /// Dates du fichier (millisecondes depuis 1970), si le système les donne.
    pub file_created: Option<i64>,
    pub file_modified: Option<i64>,
    /// Version du PDF (« 1.7 »).
    pub version: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    /// Dates écrites dans le PDF, au format « 2026-10-27T14:05:00 ».
    pub created: Option<String>,
    pub modified: Option<String>,
}

/// Rotation d'affichage choisie par l'utilisateur, dans le sens des aiguilles d'une montre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Rotation {
    #[default]
    R0,
    R90,
    R180,
    R270,
}

impl Rotation {
    /// Accepte n'importe quel nombre de degrés (multiple de 90), même négatif.
    pub fn from_degrees(degrees: i32) -> Self {
        match degrees.rem_euclid(360) {
            90 => Self::R90,
            180 => Self::R180,
            270 => Self::R270,
            _ => Self::R0,
        }
    }

    pub fn degrees(self) -> i32 {
        match self {
            Self::R0 => 0,
            Self::R90 => 90,
            Self::R180 => 180,
            Self::R270 => 270,
        }
    }

    pub fn then(self, other: Rotation) -> Rotation {
        Rotation::from_degrees(self.degrees() + other.degrees())
    }

    pub fn is_sideways(self) -> bool {
        matches!(self, Self::R90 | Self::R270)
    }
}

/// Une page telle qu'elle est rangée dans la fenêtre : de quel document elle vient,
/// son numéro dans ce document, et la rotation choisie par l'utilisateur (en degrés).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PageRef {
    pub doc: DocId,
    pub page: usize,
    #[serde(default)]
    pub rotation: i32,
    /// Annotations ajoutées sur la page (voir [`crate::annotate`]).
    #[serde(default)]
    pub annotations: Vec<Annotation>,
}

impl PageRef {
    pub fn new(doc: DocId, page: usize, rotation: i32) -> Self {
        Self { doc, page, rotation, annotations: Vec::new() }
    }
}

/// Rectangle en proportions de la page (0 à 1), origine en haut à gauche.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct NormRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl NormRect {
    /// Le même rectangle une fois la page tournée.
    pub fn rotated(self, rotation: Rotation) -> NormRect {
        let NormRect { x, y, w, h } = self;
        match rotation {
            Rotation::R0 => self,
            Rotation::R90 => NormRect { x: 1.0 - y - h, y: x, w: h, h: w },
            Rotation::R180 => NormRect { x: 1.0 - x - w, y: 1.0 - y - h, w, h },
            Rotation::R270 => NormRect { x: y, y: 1.0 - x - w, w: h, h: w },
        }
    }
}

/// Un résultat de recherche : une occurrence du texte cherché.
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub doc: DocId,
    pub page: usize,
    /// Les zones à surligner (plusieurs si le mot est coupé sur deux lignes).
    pub rects: Vec<NormRect>,
    /// La ligne où le texte a été trouvé, pour la liste des résultats.
    pub context: String,
}

/// Le texte d'une page, caractère par caractère, avec la place de chacun
/// (en proportions de la page, comme pour la recherche). Sert à sélectionner et surligner.
#[derive(Debug, Clone, Default, Serialize)]
pub struct PageText {
    pub chars: Vec<char>,
    /// Un rectangle par caractère ; vide (largeur nulle) pour les espaces et retours à la ligne ajoutés.
    pub rects: Vec<NormRect>,
}

/// Une page rendue en image, prête à être affichée.
#[derive(Debug, Clone)]
pub struct RenderedImage {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
    pub width: u32,
    pub height: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_depuis_des_degres() {
        assert_eq!(Rotation::from_degrees(-90), Rotation::R270);
        assert_eq!(Rotation::from_degrees(450), Rotation::R90);
        assert_eq!(Rotation::R270.then(Rotation::R180), Rotation::R90);
        assert!(Rotation::R90.is_sideways());
        assert!(!Rotation::R180.is_sideways());
    }

    #[test]
    fn rectangle_tourne_d_un_quart_de_tour() {
        // Un coin en haut à gauche se retrouve en haut à droite.
        let r = NormRect { x: 0.1, y: 0.0, w: 0.2, h: 0.1 }.rotated(Rotation::R90);
        assert!((r.x - 0.9).abs() < 1e-6 && (r.y - 0.1).abs() < 1e-6);
        assert_eq!((r.w, r.h), (0.1, 0.2));
        // Quatre quarts de tour : retour au départ.
        let d = NormRect { x: 0.1, y: 0.2, w: 0.3, h: 0.1 };
        let t = d.rotated(Rotation::R90).rotated(Rotation::R90).rotated(Rotation::R90).rotated(Rotation::R90);
        assert!((t.x - d.x).abs() < 1e-6 && (t.y - d.y).abs() < 1e-6);
    }
}
