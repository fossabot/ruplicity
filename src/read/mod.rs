// mod iter;
#[allow(dead_code)]
mod block;
#[allow(dead_code)]
mod cache;
#[allow(dead_code)]
mod stream;
#[allow(dead_code)]
mod volume;

use std::cmp;
use std::io::{self, BufRead, Read};
use std::path::Path;
use flate2::read::GzDecoder;
use tar::Archive;

use ::not_found;
use ::other;
use backend::Backend;
use collections::BackupChain;
use manifest::ManifestChain;
use signatures::{Chain, DiffType, Entry as SnapEntry, EntryId};
use read::block::{BLOCK_SIZE, BlockId};
use read::cache::BlockCache;
use read::stream::BlockStream;


pub struct Entry<'a, B: 'a> {
    provider: &'a BlockProvider<B>,
    entry_type: DiffType,
    buf: Box<[u8]>,
    len: usize,
    pos: usize,
    id: BlockId,
    stream: Option<Box<BlockStream + 'a>>,
}

pub struct BlockProvider<B> {
    manifests: ManifestChain,
    back: BackupChain,
    sig: Chain,
    backend: B,
    dcache: BlockCache,
    scache: BlockCache,
}


// Provides resources only for a specific entry
struct EntryResourceProxy<'a, B: 'a> {
    provider: &'a BlockProvider<B>,
    entry: EntryId,
}

#[derive(Debug, Eq, PartialEq)]
enum CacheType {
    Snapshot,
    Signature,
}


impl<'a, B: Backend> Entry<'a, B> {
    fn fill_block(&mut self) -> io::Result<()> {
        let optlen = self.provider
            .read_cached_block(self.id, &mut self.buf, CacheType::Snapshot);
        if let Some(len) = optlen {
            // the block is in cache, return it
            self.len = len;
            return Ok(());
        }
        // otherwise we need to use our block stream
        if self.stream.is_none() {
            // not present, create it now
            self.stream = Some(try!(self.provider.block_stream(self.id.0)));
        }
        let mut stream = self.stream.as_mut().unwrap();
        try!(stream.seek_to_block(self.id.1));
        self.len = try!(stream.read(&mut self.buf));
        self.pos = 0;
        Ok(())
    }
}

impl<'a, B: Backend> Read for Entry<'a, B> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.len > 0 {
            // we have buffered stuff... just copy as much as possible
            let len = cmp::min(self.len, buf.len());
            buf.copy_from_slice(&self.buf[self.pos..self.pos + len]);
            self.pos += len;
            self.len -= len;
            Ok(len)
        } else {
            // try to fill the block by using the provider
            try!(self.fill_block());
            self.id.1 += 1;
            if self.len > 0 {
                // recurse, now we are sure there's something buffered
                self.read(buf)
            } else {
                // end of the stream
                Ok(0)
            }
        }
    }
}

impl<'a, B: Backend> BufRead for Entry<'a, B> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.len == 0 {
            try!(self.fill_block());
        }
        Ok(&self.buf[self.pos..self.pos + self.len])
    }

    fn consume(&mut self, amt: usize) {
        let amt = cmp::min(amt, self.len);
        self.pos += amt;
        self.len -= amt;
    }
}


impl<B> BlockProvider<B> {
    pub fn new(manifests: ManifestChain,
               bchain: BackupChain,
               sigchain: Chain,
               backend: B,
               cache_size: usize)
               -> Self {
        BlockProvider {
            manifests: manifests,
            back: bchain,
            sig: sigchain,
            backend: backend,
            dcache: BlockCache::new((cache_size as f64 * 0.4) as usize),
            scache: BlockCache::new((cache_size as f64 * 0.6) as usize),
        }
    }

    pub fn signature_chain(&self) -> &Chain {
        &self.sig
    }
}

impl<B: Backend> BlockProvider<B> {
    pub fn read(&self, entry: EntryId) -> Option<Entry<B>> {
        Some(Entry {
            provider: &self,
            entry_type: self.sig.entry(entry).diff_type(),
            buf: Box::new([0; BLOCK_SIZE]),
            len: 0,
            pos: 0,
            id: (entry, 0),
            stream: None,
        })
    }

    fn read_cached_block(&self, id: BlockId, buf: &mut [u8], ctype: CacheType) -> Option<usize> {
        if ctype == CacheType::Snapshot {
            self.scache.read(id, buf)
        } else {
            self.dcache.read(id, buf)
        }
    }

    fn block_stream<'a>(&'a self, entry: EntryId) -> io::Result<Box<BlockStream + 'a>> {
        let sig_entry = self.sig.entry(entry);
        match sig_entry.diff_type() {
            DiffType::Snapshot => unimplemented!(),
            DiffType::Signature => unimplemented!(),
            _ => Ok(Box::new(stream::NullStream)),
        }
    }

    fn volume_of_block(&self, id: BlockId) -> Option<usize> {
        let snapnum = (id.0).1 as usize;
        let entry = self.sig.entry(id.0);
        let manifest = match self.manifests.get(snapnum) {
            Some(m) => m,
            None => {
                return None;
            }
        };
        manifest.volume_of_block(entry.path_bytes(), id.1)
    }

    fn volume<'a>(&'a self, snapnum: usize, volnum: usize) -> io::Result<Option<Archive<Box<Read + 'a>>>> {
        let backup_set = match self.back.nth_set(snapnum) {
            Some(s) => s,
            None => {
                return Err(not_found(format!("backup set #{} not found", snapnum)));
            }
        };
        let vol_path = match backup_set.volume_path(volnum) {
            Some(p) => Path::new(p),
            None => {
                return Err(not_found(format!("no path for volume #{}", volnum)));
            }
        };
        if backup_set.is_encrypted() {
            return Err(other("encrypted backups are not supported"));
        }

        let rawfile = try!(self.backend.open_file(vol_path));
        let result: Box<Read + 'a> = if backup_set.is_compressed() {
            Box::new(try!(GzDecoder::new(rawfile)))
        } else {
            Box::new(rawfile)
        };

        Ok(Some(Archive::new(result)))
    }
}

impl<'a, B: Backend + 'a> stream::Resources for EntryResourceProxy<'a, B> {
    fn snapshot_cache(&self) -> &BlockCache {
        &self.provider.scache
    }

    fn signature_cache(&self) -> &BlockCache {
        &self.provider.dcache
    }

    fn volume<'b>(&'b self, n: usize) -> io::Result<Option<Archive<Box<Read + 'b>>>> {
        self.provider.volume(self.entry.1 as usize, n)
    }

    fn volume_of_block(&self, n: usize) -> Option<usize> {
        self.provider.volume_of_block(((self.entry), n))
    }
}
