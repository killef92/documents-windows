//! Lecture des « répertoires » d'un fichier TIFF (la table des matières de chaque page),
//! sans décoder les images : pour compter les pages d'un scan et trouver l'aperçu JPEG
//! rangé dans une photo brute (CR2, DNG, qui sont des TIFF).
//!
//! Seul le TIFF classique est lu ici (pas le BigTIFF, réservé aux fichiers de plus de 4 Go).

/// Une entrée d'un répertoire : son numéro de balise et ses valeurs (entières).
pub(crate) struct Entry {
    pub tag: u16,
    pub values: Vec<u32>,
}

pub(crate) struct Dir {
    /// Position du répertoire dans le fichier.
    pub offset: u32,
    pub entries: Vec<Entry>,
}

impl Dir {
    pub fn get(&self, tag: u16) -> Option<&[u32]> {
        self.entries.iter().find(|e| e.tag == tag).map(|e| e.values.as_slice())
    }

    pub fn first(&self, tag: u16) -> Option<u32> {
        self.get(tag).and_then(|v| v.first().copied())
    }
}

pub(crate) struct Tiff<'a> {
    bytes: &'a [u8],
    big_endian: bool,
}

/// Au-delà, un fichier est sûrement abîmé (ou piégé) : on arrête de lire.
const MAX_DIRS: usize = 10_000;
const MAX_VALUES: usize = 1 << 16;

impl<'a> Tiff<'a> {
    pub fn new(bytes: &'a [u8]) -> Option<Self> {
        let big_endian = match bytes.get(0..4)? {
            b"II*\0" => false,
            b"MM\0*" => true,
            _ => return None,
        };
        Some(Self { bytes, big_endian })
    }

    pub fn u16_at(&self, at: usize) -> Option<u16> {
        let b: [u8; 2] = self.bytes.get(at..at.checked_add(2)?)?.try_into().ok()?;
        Some(if self.big_endian { u16::from_be_bytes(b) } else { u16::from_le_bytes(b) })
    }

    pub fn u32_at(&self, at: usize) -> Option<u32> {
        let b: [u8; 4] = self.bytes.get(at..at.checked_add(4)?)?.try_into().ok()?;
        Some(if self.big_endian { u32::from_be_bytes(b) } else { u32::from_le_bytes(b) })
    }

    /// Le répertoire rangé à cette position, et la position du suivant (0 s'il n'y en a pas).
    pub fn dir_at(&self, offset: u32) -> Option<(Dir, u32)> {
        let at = offset as usize;
        let count = self.u16_at(at)? as usize;
        let mut entries = Vec::with_capacity(count);
        for i in 0..count {
            let e = at + 2 + i * 12;
            let tag = self.u16_at(e)?;
            let kind = self.u16_at(e + 2)?;
            let n = self.u32_at(e + 4)? as usize;
            let size = match kind {
                1 | 6 | 7 => 1,
                3 | 8 => 2,
                4 | 9 | 13 => 4,
                5 => 8, // fraction (résolution) : gardée arrondie
                _ => continue, // textes… : rien d'utile ici
            };
            if n == 0 || n > MAX_VALUES {
                continue;
            }
            let start = if n * size <= 4 { e + 8 } else { self.u32_at(e + 8)? as usize };
            let mut values = Vec::with_capacity(n);
            for k in 0..n {
                let p = start + k * size;
                let v = match size {
                    1 => u32::from(*self.bytes.get(p)?),
                    2 => u32::from(self.u16_at(p)?),
                    4 => self.u32_at(p)?,
                    _ => {
                        let (num, den) = (self.u32_at(p)?, self.u32_at(p + 4)?);
                        if den == 0 { 0 } else { ((f64::from(num) / f64::from(den)).round()) as u32 }
                    }
                };
                values.push(v);
            }
            entries.push(Entry { tag, values });
        }
        let next = self.u32_at(at + 2 + count * 12).unwrap_or(0);
        Some((Dir { offset, entries }, next))
    }

    /// Les répertoires principaux, dans l'ordre : une page chacun pour un scan.
    pub fn main_dirs(&self) -> Vec<Dir> {
        let mut dirs: Vec<Dir> = Vec::new();
        let mut next = self.u32_at(4).unwrap_or(0);
        while next != 0 && dirs.len() < MAX_DIRS && !dirs.iter().any(|d| d.offset == next) {
            let Some((dir, after)) = self.dir_at(next) else { break };
            dirs.push(dir);
            next = after;
        }
        dirs
    }

    /// Tous les répertoires : les principaux et leurs sous-répertoires (balise SubIFDs).
    pub fn all_dirs(&self) -> Vec<Dir> {
        let mut dirs = self.main_dirs();
        let mut i = 0;
        while i < dirs.len() && dirs.len() < MAX_DIRS {
            let subs: Vec<u32> = dirs[i].get(SUB_IFDS).map(<[u32]>::to_vec).unwrap_or_default();
            for offset in subs {
                if !dirs.iter().any(|d| d.offset == offset) {
                    if let Some((dir, _)) = self.dir_at(offset) {
                        dirs.push(dir);
                    }
                }
            }
            i += 1;
        }
        dirs
    }

    /// Une copie du fichier dont la première page est celle rangée à `offset` : les lecteurs
    /// de TIFF ordinaires (qui ne lisent que la première page) montrent alors celle-ci.
    pub fn with_first_dir(&self, offset: u32) -> Vec<u8> {
        let mut copy = self.bytes.to_vec();
        let b = if self.big_endian { offset.to_be_bytes() } else { offset.to_le_bytes() };
        copy[4..8].copy_from_slice(&b);
        copy
    }
}

pub(crate) const NEW_SUBFILE_TYPE: u16 = 0xFE;
pub(crate) const COMPRESSION: u16 = 0x103;
pub(crate) const X_RESOLUTION: u16 = 0x11A;
pub(crate) const Y_RESOLUTION: u16 = 0x11B;
pub(crate) const ORIENTATION: u16 = 0x112;
pub(crate) const STRIP_OFFSETS: u16 = 0x111;
pub(crate) const STRIP_BYTE_COUNTS: u16 = 0x117;
pub(crate) const SUB_IFDS: u16 = 0x14A;
pub(crate) const JPEG_OFFSET: u16 = 0x201;
pub(crate) const JPEG_LENGTH: u16 = 0x202;
pub(crate) const DNG_VERSION: u16 = 0xC612;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Fabrique un petit TIFF (petit-boutiste) : chaque répertoire est une liste de
    /// (balise, valeurs) ; les données ajoutées après sont adressées par leur position.
    pub(crate) struct Builder {
        pub bytes: Vec<u8>,
    }

    impl Builder {
        pub fn new() -> Self {
            Self { bytes: b"II*\0\0\0\0\0".to_vec() }
        }

        /// Ajoute des octets et renvoie leur position.
        pub fn data(&mut self, data: &[u8]) -> u32 {
            if self.bytes.len() % 2 == 1 {
                self.bytes.push(0);
            }
            let at = self.bytes.len() as u32;
            self.bytes.extend_from_slice(data);
            at
        }

        /// Ajoute un répertoire (valeurs en LONG) et renvoie sa position ; `next` : celle du suivant.
        pub fn dir(&mut self, entries: &[(u16, Vec<u32>)], next: u32) -> u32 {
            let mut extra = Vec::new();
            for (_, values) in entries.iter().filter(|(_, v)| v.len() > 1) {
                extra.push(self.data(&values.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>()));
            }
            let at = self.data(&[]);
            self.bytes.extend_from_slice(&(entries.len() as u16).to_le_bytes());
            let mut extra = extra.into_iter();
            for (tag, values) in entries {
                self.bytes.extend_from_slice(&tag.to_le_bytes());
                self.bytes.extend_from_slice(&4u16.to_le_bytes());
                self.bytes.extend_from_slice(&(values.len() as u32).to_le_bytes());
                let v = if values.len() > 1 { extra.next().unwrap() } else { values[0] };
                self.bytes.extend_from_slice(&v.to_le_bytes());
            }
            self.bytes.extend_from_slice(&next.to_le_bytes());
            at
        }

        pub fn first(&mut self, offset: u32) {
            self.bytes[4..8].copy_from_slice(&offset.to_le_bytes());
        }
    }

    #[test]
    fn lit_les_repertoires_et_sous_repertoires() {
        let mut b = Builder::new();
        let sub = b.dir(&[(0x100, vec![8])], 0);
        let second = b.dir(&[(0x100, vec![4])], 0);
        let first = b.dir(&[(0x100, vec![16]), (SUB_IFDS, vec![sub]), (0x150, vec![1, 2, 3])], second);
        b.first(first);
        let tiff = Tiff::new(&b.bytes).unwrap();
        let main = tiff.main_dirs();
        assert_eq!(main.len(), 2);
        assert_eq!(main[0].first(0x100), Some(16));
        assert_eq!(main[0].get(0x150), Some(&[1, 2, 3][..]));
        assert_eq!(main[1].first(0x100), Some(4));
        assert_eq!(tiff.all_dirs().len(), 3);
        // La deuxième page devient la première de la copie.
        let copy = tiff.with_first_dir(second);
        assert_eq!(Tiff::new(&copy).unwrap().main_dirs()[0].first(0x100), Some(4));
    }

    #[test]
    fn resiste_aux_fichiers_abimes() {
        assert!(Tiff::new(b"pas un tiff").is_none());
        let mut b = Builder::new();
        b.first(4); // le répertoire pointe sur lui-même… et sur n'importe quoi
        let tiff = Tiff::new(&b.bytes).unwrap();
        assert!(tiff.main_dirs().len() <= 1);
        let mut boucle = Builder::new();
        let d = boucle.dir(&[(0x100, vec![1])], 8);
        boucle.first(d);
        let at = d as usize + 2 + 12;
        boucle.bytes[at..at + 4].copy_from_slice(&d.to_le_bytes());
        assert_eq!(Tiff::new(&boucle.bytes).unwrap().main_dirs().len(), 1);
    }
}
