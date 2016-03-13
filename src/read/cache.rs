use std::cmp;
use std::io::{self, Read, Write};
use std::sync::RwLock;
use linked_hash_map::LinkedHashMap;

use signatures::EntryId;


pub type BlockId = (EntryId, usize);

#[derive(Debug)]
pub struct BlockCache {
    // map from index to block
    // all blocks must be indexed, even unused
    index: RwLock<LinkedHashMap<BlockId, Block>>,
    max_blocks: usize,
}

#[derive(Debug)]
struct Block(Vec<u8>);


const BLOCK_SIZE: usize = 64 * 1024;


impl BlockCache {
    pub fn new(max_blocks: usize) -> Self {
        BlockCache {
            index: RwLock::new(LinkedHashMap::new()),
            max_blocks: max_blocks,
        }
    }

    pub fn read(&self, id: BlockId, buffer: &mut [u8]) -> Option<usize> {
        {
            // first refresh the block if present, by using write lock
            if self.index.write().unwrap().get_refresh(&id).is_none() {
                return None;
            }
        }

        // then read by taking the read lock only
        let index = self.index.read().unwrap();
        match index.get(&id) {
            None => None, // this can be possible even with the refresh above
            Some(block) => block.as_slice().read(buffer).ok(),
        }
    }

    pub fn write(&self, id: BlockId, buffer: &[u8]) -> Option<usize> {
        let mut index = self.index.write().unwrap();
        if index.get(&id).is_some() {
            // already written by someone else, don't change
            return None;
        }

        if index.len() >= self.max_blocks && !index.is_empty() {
            // the cache is full, reuse the least used block
            let old_block = index.pop_front().unwrap().1;
            index.insert(id, old_block);
        } else {
            // we can add another block
            index.insert(id, Block::new());
        }
        let block = index.get_mut(&id).unwrap();
        block.write_max_block(buffer).ok()
    }

    pub fn clear(&self) {
        self.index.write().unwrap().clear();
    }
}


impl Block {
    fn new() -> Self {
        Block(Vec::new())
    }

    fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        self.0.as_mut_slice()
    }

    fn write_max_block(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let buffer = &buffer[0..cmp::min(buffer.len(), BLOCK_SIZE)];
        self.0.clear();
        self.0.write(buffer)
    }
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn write_read() {
        let cache = BlockCache::new(2);
        let id = ((0, 0), 0);
        let mut buf = vec![0; 5];

        assert_eq!(cache.read(id, &mut buf), None);
        assert_eq!(cache.write(id, b"pippo"), Some(5));
        assert_eq!(cache.read(id, &mut buf), Some(5));
        assert_eq!(&buf, b"pippo");
    }

    #[test]
    fn max_blocks() {
        let cache = BlockCache::new(2);
        let id0 = ((0, 0), 0);
        let id1 = ((0, 0), 1);
        let mut buf = vec![0; 3];

        assert_eq!(cache.write(id0, b"id0"), Some(3));
        assert_eq!(cache.write(id1, b"id1"), Some(3));
        assert_eq!(cache.read(id0, &mut buf), Some(3));
        assert_eq!(&buf, b"id0");
        assert_eq!(cache.read(id1, &mut buf), Some(3));
        assert_eq!(&buf, b"id1");
    }

    #[test]
    fn full_cache() {
        let cache = BlockCache::new(2);
        let id0 = ((0, 0), 0);
        let id1 = ((0, 0), 1);
        let id2 = ((0, 0), 2);
        let mut buf = vec![0; 3];

        assert_eq!(cache.write(id0, b"id0"), Some(3));
        assert_eq!(cache.write(id1, b"id1"), Some(3));
        assert_eq!(cache.write(id2, b"id2"), Some(3));
        // id0 disappeared
        assert_eq!(cache.read(id0, &mut buf), None);
        // id1 and id2 are there
        assert_eq!(cache.read(id1, &mut buf), Some(3));
        assert_eq!(&buf, b"id1");
        assert_eq!(cache.read(id2, &mut buf), Some(3));
        assert_eq!(&buf, b"id2");
    }

    #[test]
    fn read_refresh_usage() {
        let cache = BlockCache::new(2);
        let id0 = ((0, 0), 0);
        let id1 = ((0, 0), 1);
        let id2 = ((0, 0), 2);
        let mut buf = vec![0; 3];

        assert_eq!(cache.write(id0, b"id0"), Some(3));
        assert_eq!(cache.write(id1, b"id1"), Some(3));
        // refresh id0, so id1 becomes less used
        assert_eq!(cache.read(id0, &mut buf), Some(3));
        // write another
        assert_eq!(cache.write(id2, b"id2"), Some(3));
        // id1 disappeared
        assert_eq!(cache.read(id1, &mut buf), None);
        // id0 and id2 are there
        assert_eq!(cache.read(id0, &mut buf), Some(3));
        assert_eq!(&buf, b"id0");
        assert_eq!(cache.read(id2, &mut buf), Some(3));
        assert_eq!(&buf, b"id2");
    }

    #[test]
    fn send_sync() {
        fn is_send<T: Send>(_: T) {}
        fn is_sync<T: Sync>(_: T) {}

        let cache = BlockCache::new(1);
        is_sync(&cache);
        is_send(&cache);
        is_send(cache);
    }
}