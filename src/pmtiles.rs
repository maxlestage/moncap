//! Lecture d'une archive **PMTiles v3**.
//!
//! Auto-héberger ses propres tuiles issues d'OpenStreetMap est autorisé, mais
//! une région représente des dizaines de milliers de fichiers. PMTiles règle
//! ce problème : un **fichier unique**, interrogeable par plages d'octets, que
//! des outils comme `planetiler` produisent en une commande à partir d'un
//! extrait OSM.
//!
//! Le serveur n'en lit que ce qu'il faut : l'en-tête et les répertoires sont
//! minuscules devant les données de tuiles, et sont gardés en mémoire.

use std::io::Read;

/// En-tête PMTiles v3 : 127 octets de champs à position fixe.
#[derive(Clone, Debug)]
pub struct Header {
    pub root_offset: u64,
    pub root_length: u64,
    pub leaf_offset: u64,
    pub tile_data_offset: u64,
    /// Compression des répertoires (1 = aucune, 2 = gzip).
    pub internal_compression: u8,
    /// Compression des tuiles elles-mêmes, relayée telle quelle au client.
    pub tile_compression: u8,
    /// 1 = mvt (vectoriel), 2 = png, 3 = jpeg, 4 = webp, 5 = avif.
    pub tile_type: u8,
    pub min_zoom: u8,
    pub max_zoom: u8,
}

/// Taille de l'en-tête, fixée par la spécification.
pub const HEADER_LEN: usize = 127;

impl Header {
    pub fn parse(bytes: &[u8]) -> Option<Header> {
        if bytes.len() < HEADER_LEN || &bytes[0..7] != b"PMTiles" || bytes[7] != 3 {
            return None;
        }
        let u64_at = |i: usize| {
            let mut b = [0u8; 8];
            b.copy_from_slice(&bytes[i..i + 8]);
            u64::from_le_bytes(b)
        };
        Some(Header {
            root_offset: u64_at(8),
            root_length: u64_at(16),
            leaf_offset: u64_at(40),
            tile_data_offset: u64_at(56),
            internal_compression: bytes[97],
            tile_compression: bytes[98],
            tile_type: bytes[99],
            min_zoom: bytes[100],
            max_zoom: bytes[101],
        })
    }

    /// Type MIME correspondant au contenu des tuiles.
    pub fn content_type(&self) -> &'static str {
        match self.tile_type {
            1 => "application/vnd.mapbox-vector-tile",
            2 => "image/png",
            3 => "image/jpeg",
            4 => "image/webp",
            5 => "image/avif",
            _ => "application/octet-stream",
        }
    }

    /// `Content-Encoding` des tuiles telles que stockées (2 = gzip,
    /// 3 = brotli, 4 = zstd) ; `None` pour des tuiles non compressées (1) ou
    /// de compression inconnue (0).
    pub fn content_encoding(&self) -> Option<&'static str> {
        match self.tile_compression {
            2 => Some("gzip"),
            3 => Some("br"),
            4 => Some("zstd"),
            _ => None,
        }
    }
}

/// Une entrée de répertoire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub tile_id: u64,
    pub offset: u64,
    pub length: u32,
    /// 0 = l'entrée pointe vers un répertoire feuille, pas vers une tuile.
    pub run_length: u32,
}

/// Résultat d'une recherche dans un répertoire.
pub enum Found {
    /// Tuile trouvée : décalage relatif aux données et longueur.
    Tile {
        offset: u64,
        length: u32,
    },
    /// Il faut lire un répertoire feuille avant de conclure.
    Leaf {
        offset: u64,
        length: u32,
    },
    Missing,
}

/// Décode un entier à longueur variable (base 128, petit-boutiste).
fn varint(bytes: &[u8], pos: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    let mut shift = 0u32;
    loop {
        let byte = *bytes.get(*pos)?;
        *pos += 1;
        value |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
        // Au-delà, la valeur ne tiendrait pas dans un u64 : donnée corrompue.
        if shift > 63 {
            return None;
        }
    }
}

/// Décode un répertoire (déjà décompressé).
///
/// Les champs sont stockés par colonnes et différentiels : identifiants en
/// delta, et un décalage nul signifie « colle à l'entrée précédente ».
pub fn parse_directory(bytes: &[u8]) -> Option<Vec<Entry>> {
    let mut pos = 0usize;
    let count = varint(bytes, &mut pos)? as usize;
    // Garde-fou : un répertoire annonçant des millions d'entrées sur quelques
    // octets est corrompu, et allouerait pour rien.
    if count > 10_000_000 {
        return None;
    }
    let mut entries = vec![
        Entry {
            tile_id: 0,
            offset: 0,
            length: 0,
            run_length: 0,
        };
        count
    ];
    let mut last = 0u64;
    for e in entries.iter_mut() {
        last += varint(bytes, &mut pos)?;
        e.tile_id = last;
    }
    for e in entries.iter_mut() {
        e.run_length = varint(bytes, &mut pos)? as u32;
    }
    for e in entries.iter_mut() {
        e.length = varint(bytes, &mut pos)? as u32;
    }
    for i in 0..count {
        let raw = varint(bytes, &mut pos)?;
        entries[i].offset = if raw == 0 {
            // 0 = juste après l'entrée précédente.
            let prev = entries.get(i.wrapping_sub(1))?;
            prev.offset + prev.length as u64
        } else {
            raw - 1
        };
    }
    Some(entries)
}

/// Cherche une tuile dans un répertoire.
pub fn find(entries: &[Entry], tile_id: u64) -> Found {
    // Dernière entrée dont l'identifiant est <= celui cherché.
    let idx = match entries.binary_search_by(|e| e.tile_id.cmp(&tile_id)) {
        Ok(i) => Some(i),
        Err(0) => None,
        Err(i) => Some(i - 1),
    };
    let Some(i) = idx else { return Found::Missing };
    let e = entries[i];
    if e.run_length == 0 {
        return Found::Leaf {
            offset: e.offset,
            length: e.length,
        };
    }
    // Une entrée peut couvrir plusieurs tuiles identiques consécutives.
    if tile_id < e.tile_id + e.run_length as u64 {
        Found::Tile {
            offset: e.offset,
            length: e.length,
        }
    } else {
        Found::Missing
    }
}

/// Identifiant d'une tuile dans l'ordre de la courbe de Hilbert, qui range
/// les tuiles voisines à des positions voisines — d'où des lectures groupées.
pub fn tile_id(z: u8, x: u32, y: u32) -> u64 {
    // Somme des tuiles de tous les zooms inférieurs : (4^z - 1) / 3.
    let base = ((1u64 << (2 * z as u64)) - 1) / 3;
    let mut rx: u64;
    let mut ry: u64;
    let mut d = 0u64;
    let mut x = x as u64;
    let mut y = y as u64;
    let mut s = 1u64 << (z as u64) >> 1;
    while s > 0 {
        rx = u64::from(x & s > 0);
        ry = u64::from(y & s > 0);
        d += s * s * ((3 * rx) ^ ry);
        // Rotation du quadrant, cœur de la courbe de Hilbert.
        if ry == 0 {
            if rx == 1 {
                x = s.wrapping_sub(1).wrapping_sub(x);
                y = s.wrapping_sub(1).wrapping_sub(y);
            }
            std::mem::swap(&mut x, &mut y);
        }
        s >>= 1;
    }
    base + d
}

/// Décompresse un répertoire selon le mode déclaré dans l'en-tête.
pub fn decompress(bytes: &[u8], compression: u8) -> Option<Vec<u8>> {
    match compression {
        // 1 = aucune compression.
        1 => Some(bytes.to_vec()),
        // 2 = gzip, le cas courant.
        2 => {
            let mut out = Vec::new();
            flate2::read::GzDecoder::new(bytes)
                .read_to_end(&mut out)
                .ok()?;
            Some(out)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode un entier à longueur variable (miroir de `varint`), pour
    /// fabriquer des répertoires de test.
    fn put_varint(out: &mut Vec<u8>, mut value: u64) {
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            out.push(byte);
            if value == 0 {
                return;
            }
        }
    }

    fn encode_directory(entries: &[Entry]) -> Vec<u8> {
        let mut out = Vec::new();
        put_varint(&mut out, entries.len() as u64);
        let mut last = 0u64;
        for e in entries {
            put_varint(&mut out, e.tile_id - last);
            last = e.tile_id;
        }
        for e in entries {
            put_varint(&mut out, e.run_length as u64);
        }
        for e in entries {
            put_varint(&mut out, e.length as u64);
        }
        for (i, e) in entries.iter().enumerate() {
            let contiguous =
                i > 0 && e.offset == entries[i - 1].offset + entries[i - 1].length as u64;
            put_varint(&mut out, if contiguous { 0 } else { e.offset + 1 });
        }
        out
    }

    #[test]
    fn varint_round_trip() {
        for value in [0u64, 1, 127, 128, 300, 16_384, u32::MAX as u64, u64::MAX] {
            let mut buf = Vec::new();
            put_varint(&mut buf, value);
            let mut pos = 0;
            assert_eq!(varint(&buf, &mut pos), Some(value), "valeur {value}");
            assert_eq!(pos, buf.len(), "octets non consommés pour {value}");
        }
    }

    /// Un varint qui ne se termine jamais ne doit pas boucler sans fin.
    #[test]
    fn varint_rejects_corrupt_input() {
        let mut pos = 0;
        assert_eq!(varint(&[0xff; 12], &mut pos), None);
    }

    /// Valeurs de référence de la spécification : l'ordre de Hilbert range la
    /// tuile 0/0/0 en premier, puis le zoom 1 dans le sens anti-horaire.
    #[test]
    fn hilbert_ids_match_spec() {
        assert_eq!(tile_id(0, 0, 0), 0);
        assert_eq!(tile_id(1, 0, 0), 1);
        assert_eq!(tile_id(1, 0, 1), 2);
        assert_eq!(tile_id(1, 1, 1), 3);
        assert_eq!(tile_id(1, 1, 0), 4);
        assert_eq!(tile_id(2, 0, 0), 5);
        // Le zoom 2 occupe les identifiants 5 à 20, le zoom 3 commence à 21.
        assert_eq!(tile_id(3, 0, 0), 21);
    }

    /// Deux tuiles distinctes n'ont jamais le même identifiant, à tous les
    /// zooms utiles : une collision servirait la mauvaise tuile.
    #[test]
    fn hilbert_ids_are_unique() {
        for z in 0..=6u8 {
            let side = 1u32 << z;
            let mut seen = std::collections::HashSet::new();
            for x in 0..side {
                for y in 0..side {
                    assert!(seen.insert(tile_id(z, x, y)), "collision en {z}/{x}/{y}");
                }
            }
            assert_eq!(seen.len(), (side as usize) * (side as usize));
        }
    }

    #[test]
    fn directory_round_trip_and_lookup() {
        let entries = vec![
            Entry {
                tile_id: 0,
                offset: 0,
                length: 10,
                run_length: 1,
            },
            // Contiguë : encodée avec un décalage nul.
            Entry {
                tile_id: 1,
                offset: 10,
                length: 20,
                run_length: 3,
            },
            Entry {
                tile_id: 100,
                offset: 500,
                length: 30,
                run_length: 0,
            },
        ];
        let decoded = parse_directory(&encode_directory(&entries)).expect("décodage");
        assert_eq!(decoded, entries);

        match find(&decoded, 0) {
            Found::Tile { offset, length } => assert_eq!((offset, length), (0, 10)),
            _ => panic!("tuile 0 introuvable"),
        }
        // Couverte par la plage de l'entrée 1 (identifiants 1 à 3).
        match find(&decoded, 3) {
            Found::Tile { offset, length } => assert_eq!((offset, length), (10, 20)),
            _ => panic!("tuile 3 introuvable"),
        }
        // Hors plage : absente, et surtout pas rattachée à l'entrée suivante.
        assert!(matches!(find(&decoded, 50), Found::Missing));
        // run_length nul : renvoie vers un répertoire feuille.
        assert!(matches!(find(&decoded, 100), Found::Leaf { .. }));
        // Avant la première entrée.
        assert!(matches!(find(&[], 0), Found::Missing));
    }

    #[test]
    fn header_parses_and_rejects_foreign_files() {
        let mut bytes = vec![0u8; HEADER_LEN];
        bytes[0..7].copy_from_slice(b"PMTiles");
        bytes[7] = 3;
        bytes[8..16].copy_from_slice(&127u64.to_le_bytes()); // root_offset
        bytes[16..24].copy_from_slice(&42u64.to_le_bytes()); // root_length
        bytes[56..64].copy_from_slice(&1000u64.to_le_bytes()); // tile_data_offset
        bytes[97] = 2; // répertoires gzip
        bytes[98] = 1; // tuiles non compressées
        bytes[99] = 2; // png
        bytes[101] = 14; // zoom max

        let h = Header::parse(&bytes).expect("en-tête valide");
        assert_eq!(h.root_offset, 127);
        assert_eq!(h.root_length, 42);
        assert_eq!(h.tile_data_offset, 1000);
        assert_eq!(h.content_type(), "image/png");
        assert_eq!(h.max_zoom, 14);

        assert!(Header::parse(b"pas une archive").is_none());
        let mut wrong_version = bytes.clone();
        wrong_version[7] = 2;
        assert!(Header::parse(&wrong_version).is_none(), "v2 non gérée");
    }

    #[test]
    fn gzip_directories_are_decompressed() {
        use flate2::write::GzEncoder;
        use std::io::Write;
        let entries = vec![Entry {
            tile_id: 7,
            offset: 3,
            length: 9,
            run_length: 1,
        }];
        let raw = encode_directory(&entries);
        let mut encoder = GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(&raw).unwrap();
        let gz = encoder.finish().unwrap();

        assert_eq!(decompress(&gz, 2).as_deref(), Some(raw.as_slice()));
        assert_eq!(decompress(&raw, 1).as_deref(), Some(raw.as_slice()));
        // Compression non gérée (brotli, zstd) : refus explicite.
        assert!(decompress(&raw, 3).is_none());
    }

    #[test]
    fn le_codage_des_tuiles_suit_leur_compression() {
        let mut bytes = vec![0u8; HEADER_LEN];
        bytes[0..7].copy_from_slice(b"PMTiles");
        bytes[7] = 3;
        for (compression, codage) in [
            (0, None),
            (1, None),
            (2, Some("gzip")),
            (3, Some("br")),
            (4, Some("zstd")),
        ] {
            bytes[98] = compression;
            let h = Header::parse(&bytes).unwrap();
            assert_eq!(h.content_encoding(), codage, "{compression}");
        }
    }
}
