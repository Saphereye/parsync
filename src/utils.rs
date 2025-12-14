// TODO use this for sync

// /// Represents a hash of a file chunk.
// #[derive(Debug, Clone, PartialEq, Eq)]
// pub struct ChunkHash {
//     pub index: usize,
//     pub offset: u64,
//     pub size: usize,
//     pub hash: blake3::Hash,
// }
//
// /// Computes blake3 hashes for each chunk of a file.
// /// Returns a Vec of ChunkHash, one per chunk.
// /// `chunk_size` should be a power of two (e.g., 1 << 20 for 1 MiB).
// pub fn hash_file_chunks<P: AsRef<std::path::Path>>(
//     path: P,
//     chunk_size: usize,
// ) -> std::io::Result<Vec<ChunkHash>> {
//     use std::fs::File;
//     use std::io::Read;
//
//     let mut file = File::open(path)?;
//     let mut hashes = Vec::new();
//     let mut buf = vec![0u8; chunk_size];
//     let mut offset = 0u64;
//     let mut index = 0;
//
//     loop {
//         let n = file.read(&mut buf)?;
//         if n == 0 {
//             break;
//         }
//         let hash = blake3::hash(&buf[..n]);
//         hashes.push(ChunkHash {
//             index,
//             offset,
//             size: n,
//             hash,
//         });
//         offset += n as u64;
//         index += 1;
//     }
//     Ok(hashes)
// }
