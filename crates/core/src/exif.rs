//! Pivoter une photo JPEG sans perte : on change seulement l'orientation enregistrée
//! dans ses informations EXIF (la photo elle-même n'est pas recompressée).

use crate::Rotation;

/// Orientation EXIF (1, 6, 3, 8) → rotation à appliquer pour afficher la photo droite.
pub(crate) fn rotation_of(orientation: u16) -> Option<Rotation> {
    match orientation {
        1 => Some(Rotation::R0),
        6 => Some(Rotation::R90),
        3 => Some(Rotation::R180),
        8 => Some(Rotation::R270),
        _ => None, // photos « miroir » : pas gérées sans recompression
    }
}

pub(crate) fn orientation_of(rotation: Rotation) -> u16 {
    match rotation {
        Rotation::R0 => 1,
        Rotation::R90 => 6,
        Rotation::R180 => 3,
        Rotation::R270 => 8,
    }
}

fn u16_at(data: &[u8], at: usize, big: bool) -> Option<u16> {
    let b = data.get(at..at + 2)?;
    Some(if big { u16::from_be_bytes([b[0], b[1]]) } else { u16::from_le_bytes([b[0], b[1]]) })
}

fn u32_at(data: &[u8], at: usize, big: bool) -> Option<u32> {
    let b = data.get(at..at + 4)?;
    let b = [b[0], b[1], b[2], b[3]];
    Some(if big { u32::from_be_bytes(b) } else { u32::from_le_bytes(b) })
}

/// Position de la valeur d'orientation dans le fichier, si elle existe.
fn find_orientation(jpeg: &[u8]) -> Result<Option<(usize, bool)>, ()> {
    if !jpeg.starts_with(&[0xFF, 0xD8]) {
        return Err(());
    }
    let mut at = 2;
    while at + 4 <= jpeg.len() && jpeg[at] == 0xFF {
        let marker = jpeg[at + 1];
        let len = u16::from_be_bytes([jpeg[at + 2], jpeg[at + 3]]) as usize;
        if marker == 0xDA {
            break; // début de l'image : plus d'en-têtes après
        }
        let seg = at + 4;
        if marker == 0xE1 && jpeg.get(seg..seg + 6) == Some(b"Exif\0\0") {
            let tiff = seg + 6;
            let big = match jpeg.get(tiff..tiff + 2) {
                Some(b"MM") => true,
                Some(b"II") => false,
                _ => return Err(()),
            };
            let ifd = tiff + u32_at(jpeg, tiff + 4, big).ok_or(())? as usize;
            let count = u16_at(jpeg, ifd, big).ok_or(())? as usize;
            for i in 0..count {
                let entry = ifd + 2 + i * 12;
                if u16_at(jpeg, entry, big).ok_or(())? == 0x0112 {
                    return Ok(Some((entry + 8, big)));
                }
            }
            return Err(()); // EXIF présent mais sans orientation : on ne réécrit pas le bloc
        }
        at = seg + len - 2;
    }
    Ok(None)
}

/// Renvoie le JPEG avec la nouvelle orientation, ou `None` s'il faut passer par une recompression.
pub(crate) fn with_orientation(jpeg: &[u8], orientation: u16) -> Option<Vec<u8>> {
    match find_orientation(jpeg).ok()? {
        Some((at, big)) => {
            let mut out = jpeg.to_vec();
            let v = if big { orientation.to_be_bytes() } else { orientation.to_le_bytes() };
            out[at..at + 2].copy_from_slice(&v);
            Some(out)
        }
        None => {
            // Pas d'EXIF : on ajoute un petit bloc qui ne contient que l'orientation.
            let mut app1 = Vec::new();
            app1.extend_from_slice(b"Exif\0\0MM\0\x2a\0\0\0\x08\0\x01");
            app1.extend_from_slice(&[0x01, 0x12, 0x00, 0x03, 0, 0, 0, 1]);
            app1.extend_from_slice(&orientation.to_be_bytes());
            app1.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
            let mut out = Vec::with_capacity(jpeg.len() + app1.len() + 4);
            out.extend_from_slice(&jpeg[..2]);
            out.extend_from_slice(&[0xFF, 0xE1]);
            out.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
            out.extend_from_slice(&app1);
            out.extend_from_slice(&jpeg[2..]);
            Some(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correspondance_orientation_rotation() {
        for r in [Rotation::R0, Rotation::R90, Rotation::R180, Rotation::R270] {
            assert_eq!(rotation_of(orientation_of(r)), Some(r));
        }
        assert_eq!(rotation_of(2), None);
    }

    #[test]
    fn refuse_ce_qui_n_est_pas_un_jpeg() {
        assert!(with_orientation(b"pas un jpeg", 6).is_none());
    }
}
