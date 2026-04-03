/*
Persist compact block filter on disk.
When a filter is retrieved, it is persisted.
When a filter is about to be retrieved, first it is checked from storage.
Filters are indexed by height. and only deep-buried filters are persisted.
Filters are written to disk append-only, with an in-memory index by height.
 */

/// TODO:
/// Header size in const
use bip157_store::filter_store_trait::FilterStoreTrait;

use hex_conservative::DisplayHex;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Stores variable-length byte-array filters, each identified by a unique `height`.
///
/// Layout of the storage file (little-endian):
///   [blockhash: 32 bytes][height: u32][length: u32][data: <length> bytes]  (repeated)
#[derive(Debug)]
pub struct FilterStore {
    file: Option<File>,
    /// height → (byte offset of the entry's data, data length)
    index: HashMap<u32, (u64, u32)>,
}

impl FilterStore {
    /// Scan the file from the beginning and populate `self.index`.
    /// Return the number of entries
    fn rebuild_index(&mut self) -> io::Result<usize> {
        let mut file = if let Some(file) = &self.file {
            file
        } else {
            return Ok(0);
        };
        file.seek(SeekFrom::Start(0))?;
        let mut header = [0u8; 40];

        loop {
            let offset = file.stream_position()?;
            match file.read_exact(&mut header) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            }

            let _hash: [u8; 32] = header[0..32].try_into().unwrap();
            let height = u32::from_le_bytes(header[32..36].try_into().unwrap());
            let length = u32::from_le_bytes(header[36..40].try_into().unwrap());
            // println!("item:   h {}  l {}  o {}", height, length, offset);

            self.index.insert(height, (offset, length));

            // Skip over the data payload.
            file.seek(SeekFrom::Current(length as i64))?;
        }

        println!("Storage file read, {} items indexed", self.count());

        // self.internal_test();
        Ok(self.count())
    }

    /*
    fn internal_test(&mut self) {
        println!("Internal test: {}", self.count());
        for (height, (offset, length)) in &self.index {
            println!("item:   h {}  l {}  o {}", height, length, offset);
        }
        // for (height, (offset, length)) in &self.index {
        //     let (offset, len1) = self.index.get(&height).unwrap();
        //     println!("item:   h {}  l {}  o {}", height, len1, offset);
        // }
        let heights: Vec<_> = self.heights().collect();
        for height in heights {
            println!("item:   h {}", height);
            let (_block_hash, data) = self.get(height).unwrap().unwrap();
            println!("item:   h {}  l {}", height, data.len());
        }
        println!("Internal end");
    }
    */
}

impl FilterStoreTrait for FilterStore {
    /// Open (or create) a store at `path`, rebuilding the in-memory index.
    fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        let mut store = FilterStore {
            file: Some(file),
            index: HashMap::new(),
        };
        let _count = store.rebuild_index()?;
        Ok(store)
    }

    /// Append a filter and record its position in the index.
    /// Returns an error if `height` already exists.
    fn add(
        &mut self,
        hash: &[u8],
        height: u32,
        header_tip_height: u32,
        filter: &[u8],
    ) -> io::Result<()> {
        let mut file = if let Some(file) = &self.file {
            file
        } else {
            return Ok(());
        };
        let depth = header_tip_height as i32 - height as i32;
        if depth < 50 {
            // TODO configurable
            println!(
                "Warning: filter is not confirmed deep enough, not storing {} {} {}",
                height, header_tip_height, depth
            );
            return Ok(());
        }
        if self.index.contains_key(&height) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("height {height} already exists"),
            ));
        }

        println!(
            "Adding filter to store: {}/{} {} {}",
            height,
            header_tip_height,
            hash.to_lower_hex_string(),
            filter.len()
        );

        // Seek to end to get the offset where the data payload will start.
        let entry_start = file.seek(SeekFrom::End(0))?;
        // let data_offset = entry_start + 40; // skip the 40-byte header

        if hash.len() != 32 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid hash length".to_string(),
            ));
        }
        let length = filter.len() as u32;
        file.write_all(hash)?;
        file.write_all(&height.to_le_bytes())?;
        file.write_all(&length.to_le_bytes())?;
        file.write_all(filter)?;
        file.flush()?;

        self.index.insert(height, (entry_start, length));
        Ok(())
    }

    /// Retrieve the filter data for `height`, or `None` if not found.
    fn get(&mut self, height: u32) -> io::Result<Option<([u8; 32], Vec<u8>)>> {
        let mut file = if let Some(file) = &self.file {
            file
        } else {
            return Ok(None);
        };
        let Some(&(offset, length)) = self.index.get(&height) else {
            return Ok(None);
        };

        file.seek(SeekFrom::Start(offset))?;
        // let mut block_hash = vec![0u8; 40];
        // self.file.read_exact(&mut block_hash)?;
        let mut header = vec![0u8; 40];
        file.read_exact(&mut header)?;
        let block_hash = header[0..32].to_vec();
        let data_length = length;
        let mut data = vec![0u8; data_length as usize];
        // println!("data_len {}", data_length);
        file.read_exact(&mut data)?;
        Ok(Some((block_hash.try_into().unwrap(), data)))
    }

    fn count(&self) -> usize {
        self.index.len()
    }

    fn total_size(&self) -> u64 {
        let mut s = 0u64;
        for (_offset, l) in self.index.values() {
            s += *l as u64;
        }
        s
    }

    // /// Returns all heights present in the store (unordered).
    // pub fn heights(&self) -> impl Iterator<Item = u32> + '_ {
    //     self.index.keys().copied()
    // }

    // /// Returns `(height, data_length)` for every stored filter (unordered).
    // pub fn entries(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
    //     self.index.iter().map(|(&height, &(_offset, length))| (height, length))
    // }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn temp_store() -> (FilterStore, NamedTempFile) {
        let f = NamedTempFile::new().unwrap();
        let store = FilterStore::open(f.path()).unwrap();
        (store, f)
    }

    #[test]
    fn add_and_get_single_filter() {
        let (mut store, _f) = temp_store();
        let hash = [0u8; 32];
        let data: Vec<u8> = (0..100).collect();
        store.add(&hash, 42, 1042, &data).unwrap();
        assert_eq!(store.get(42).unwrap().unwrap().1, data);
    }

    #[test]
    fn get_missing_returns_none() {
        let (mut store, _f) = temp_store();
        assert_eq!(store.get(99).unwrap(), None);
    }

    #[test]
    fn duplicate_height_is_rejected() {
        let (mut store, _f) = temp_store();
        let hash = [0u8; 32];
        store.add(&hash, 1, 2001, b"hello").unwrap();
        let err = store.add(&hash, 1, 2001, b"world").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn multiple_filters_roundtrip() {
        let (mut store, _f) = temp_store();

        let filters: Vec<(u32, Vec<u8>, [u8; 32])> = (0u8..10)
            .map(|h| (h as u32, vec![h; (h as usize + 1) * 1000], [h; 32]))
            .collect();

        for (h, data, hash) in &filters {
            store.add(hash, *h, *h + 1000, data).unwrap();
        }

        for (h, expected, hash) in &filters {
            assert_eq!(store.get(*h).unwrap().unwrap(), (*hash, expected.clone()));
        }
    }

    #[test]
    fn index_rebuilt_after_reopen() {
        let f = NamedTempFile::new().unwrap();

        {
            let mut store = FilterStore::open(f.path()).unwrap();
            store.add(&[1u8; 32], 7, 107, b"filter seven").unwrap();
            store
                .add(&[2u8; 32], 99, 199, b"filter ninety-nine")
                .unwrap();
        }

        // Reopen — index must be rebuilt from disk.
        let mut store = FilterStore::open(f.path()).unwrap();
        assert_eq!(store.get(7).unwrap().unwrap().1, b"filter seven" as &[u8]);
        assert_eq!(
            store.get(99).unwrap().unwrap().1,
            b"filter ninety-nine" as &[u8]
        );
    }

    #[test]
    fn large_filter_roundtrip() {
        let (mut store, _f) = temp_store();
        // 30 000-byte filter, typical real-world size
        let data: Vec<u8> = (0u32..30_000).map(|i| (i % 251) as u8).collect();
        store.add(&[0u8; 32], 1_000_000, 1_001_000, &data).unwrap();
        assert_eq!(store.get(1_000_000).unwrap().unwrap().1, data);
    }

    /*
    #[test]
    fn heights_iterator() {
        let (mut store, _f) = temp_store();
        store.add(10, b"a").unwrap();
        store.add(20, b"b").unwrap();
        store.add(30, b"c").unwrap();

        let mut heights: Vec<u32> = store.heights().collect();
        heights.sort();
        assert_eq!(heights, vec![10, 20, 30]);
    }
    */
}
