//! Operations on manifest files.

use std::cmp::Ordering;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::io::{self, BufRead};
use std::num::ParseIntError;
use std::path::Path;
use std::str::{self, FromStr, Utf8Error};
use std::usize;

use rawpath::RawPath;


/// Manifest file info.
#[derive(Debug)]
pub struct Manifest {
    hostname: String,
    local_dir: RawPath,
    volumes: Vec<Volume>,
}

/// Volume info.
#[derive(Debug)]
pub struct Volume {
    start_path: PathBlock,
    end_path: PathBlock,
    hash_type: String,
    hash: Vec<u8>,
}

/// wip
#[derive(Debug)]
pub enum ParseError {
    /// wip
    Io(io::Error),
    /// wip
    MissingKeyword(String),
    /// wip
    MissingHash,
    /// wip
    MissingHashType,
    /// wip
    MissingPath,
    /// wip
    OutOfOrderVolume(usize),
    /// wip
    ParseInt(ParseIntError),
    /// wip
    Utf8(Utf8Error),
}


#[derive(Debug)]
struct PathBlock {
    path: RawPath,
    block: Option<usize>,
}

struct ManifestParser<R> {
    input: R,
    buf: Vec<u8>,
}

struct WordIter<'a>(&'a [u8]);


impl Manifest {
    /// Parses a stream to get a manifest.
    pub fn parse<R: BufRead>(m: &mut R) -> Result<Self, ParseError> {
        let parser = ManifestParser::new(m);
        parser.parse()
    }

    /// wip
    pub fn hostname(&self) -> Option<&str> {
        Some(&self.hostname)
    }

    /// wip
    pub fn local_dir(&self) -> Option<&Path> {
        self.local_dir.as_path()
    }

    /// wip
    pub fn max_vol_num(&self) -> usize {
        self.volumes.len()
    }

    /// wip
    pub fn volume(&self, num: usize) -> Option<&Volume> {
        if num == 0 { None } else { self.volumes.get(num - 1) }
    }

    /// wip
    pub fn first_volume_of_path(&self, path: &[u8]) -> Option<usize> {
        self.volumes
            .binary_search_by(|v| {
                match path.cmp(v.start_path_bytes()) {
                    Ordering::Less => Ordering::Greater,
                    Ordering::Greater => {
                        match path.cmp(v.end_path_bytes()) {
                            Ordering::Less | Ordering::Equal => Ordering::Equal,
                            Ordering::Greater => Ordering::Less,
                        }
                    }
                    Ordering::Equal => {
                        match v.start_path.block {
                            Some(n) if n > 0 => Ordering::Greater,
                            _ => Ordering::Equal,
                        }
                    }
                }
            })
            .map(|idx| idx + 1)
            .ok()
    }
}


impl Volume {
    /// wip
    pub fn start_path(&self) -> Option<&Path> {
        self.start_path.path.as_path()
    }

    /// wip
    pub fn end_path(&self) -> Option<&Path> {
        self.end_path.path.as_path()
    }

    /// wip
    pub fn start_path_bytes(&self) -> &[u8] {
        self.start_path.path.as_bytes()
    }

    /// wip
    pub fn end_path_bytes(&self) -> &[u8] {
        self.end_path.path.as_bytes()
    }

    /// wip
    pub fn start_block(&self) -> Option<usize> {
        self.start_path.block
    }

    /// wip
    pub fn end_block(&self) -> Option<usize> {
        self.end_path.block
    }

    /// wip
    pub fn hash_type(&self) -> &str {
        &self.hash_type
    }

    /// wip
    pub fn hash(&self) -> &[u8] {
        &self.hash
    }
}


impl Error for ParseError {
    fn description(&self) -> &str {
        match *self {
            ParseError::Io(ref err) => err.description(),
            ParseError::MissingKeyword(_) => "missing keyword in manifest",
            ParseError::MissingHash => "missing required hash",
            ParseError::MissingHashType => "missing required hash type",
            ParseError::MissingPath => "missing required path",
            ParseError::OutOfOrderVolume(_) => "a volume is missing or volumes are unsorted",
            ParseError::ParseInt(ref err) => err.description(),
            ParseError::Utf8(ref err) => err.description(),
        }
    }
}

impl Display for ParseError {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        match *self {
            ParseError::Io(ref e) => write!(fmt, "{}", e),
            ParseError::MissingKeyword(ref e) => write!(fmt, "missing keyword '{}' in manifest", e),
            ParseError::OutOfOrderVolume(v) => {
                write!(fmt, "volumes are not sorted around volume {}", v)
            }
            ParseError::ParseInt(ref e) => write!(fmt, "{}", e),
            ParseError::Utf8(ref e) => write!(fmt, "{}", e),
            _ => write!(fmt, "{}", self.description()),
        }
    }
}

impl From<io::Error> for ParseError {
    fn from(err: io::Error) -> ParseError {
        ParseError::Io(err)
    }
}

impl From<ParseIntError> for ParseError {
    fn from(err: ParseIntError) -> ParseError {
        ParseError::ParseInt(err)
    }
}

impl From<Utf8Error> for ParseError {
    fn from(err: Utf8Error) -> ParseError {
        ParseError::Utf8(err)
    }
}


macro_rules! check_eof(
    ($e:expr) => (
        if !try!($e) {
            return Err(From::from(io::Error::new(io::ErrorKind::UnexpectedEof,
                                                 "file ends unexpectedly")));
        }
    )
);


impl<R: BufRead> ManifestParser<R> {
    pub fn new(input: R) -> Self {
        ManifestParser {
            input: input,
            buf: vec![],
        }
    }

    pub fn parse(mut self) -> Result<Manifest, ParseError> {
        check_eof!(self.read_line());
        let hostname = try!(self.read_param_str("Hostname"));
        check_eof!(self.read_line());
        let local_dir = RawPath::from_bytes(try!(self.read_param_bytes("Localdir")));

        let mut volumes = Vec::new();
        while let Some((vol, i)) = try!(self.read_volume()) {
            // check if out of order
            if i != volumes.len() + 1 {
                return Err(ParseError::OutOfOrderVolume(i));
            }
            volumes.push(vol);
        }

        Ok(Manifest {
            hostname: hostname,
            local_dir: local_dir,
            volumes: volumes,
        })
    }

    fn read_volume(&mut self) -> Result<Option<(Volume, usize)>, ParseError> {
        if !try!(self.read_line()) {
            // EOF
            return Ok(None);
        }

        // volume number
        let mut param = try!(self.read_param_str("Volume"));
        if param.ends_with(":") {
            param.pop();
        }
        let num = try!(usize::from_str(&param));
        check_eof!(self.read_line());
        let start_path = try!(self.read_path_block("StartingPath"));
        check_eof!(self.read_line());
        let end_path = try!(self.read_path_block("EndingPath"));
        check_eof!(self.read_line());
        let (htype, h) = try!(self.read_hash_param());

        let vol = Volume {
            start_path: start_path,
            end_path: end_path,
            hash_type: htype,
            hash: h,
        };
        Ok(Some((vol, num)))
    }

    fn read_line(&mut self) -> io::Result<bool> {
        self.buf.clear();
        let mut len = try!(self.input.read_until(b'\n', &mut self.buf));
        if len > 0 && self.buf[len - 1] == b'\n' {
            len -= 1;
        }
        if len > 0 && self.buf[len - 1] == b'\r' {
            len -= 1;
        }
        self.buf.truncate(len);

        Ok(!self.buf.is_empty())
    }

    fn read_param_bytes(&mut self, key: &str) -> Result<Vec<u8>, ParseError> {
        let mut words = WordIter(&self.buf);
        let kw = match words.next() {
            Some(word) => try!(str::from_utf8(word)),
            None => "",
        };
        if kw != key {
            return Err(ParseError::MissingKeyword(key.to_owned()));
        }
        let param = match words.next() {
            Some(word) => word,
            None => {
                return Ok(vec![]);
            }
        };
        Ok(unescape(param))
    }

    fn read_param_str(&mut self, key: &str) -> Result<String, ParseError> {
        let bytes = try!(self.read_param_bytes(key));
        String::from_utf8(bytes).map_err(|e| From::from(e.utf8_error()))
    }

    fn read_path_block(&mut self, key: &str) -> Result<PathBlock, ParseError> {
        let mut words = WordIter(&self.buf);
        let kw = match words.next() {
            Some(word) => try!(str::from_utf8(word)),
            None => "",
        };
        if kw != key {
            return Err(ParseError::MissingKeyword(key.to_owned()));
        }
        let path = match words.next() {
            Some(word) => RawPath::from_bytes(unescape(word)),
            None => {
                return Err(ParseError::MissingPath);
            }
        };
        let block = match words.next() {
            Some(word) => {
                let s = try!(str::from_utf8(word));
                Some(try!(usize::from_str(s)))
            }
            None => None,
        };

        Ok(PathBlock {
            path: path,
            block: block,
        })
    }

    fn read_hash_param(&mut self) -> Result<(String, Vec<u8>), ParseError> {
        let mut words = WordIter(&self.buf);
        let kw = match words.next() {
            Some(word) => try!(str::from_utf8(word)),
            None => "",
        };
        if kw != "Hash" {
            return Err(ParseError::MissingKeyword("Hash".to_owned()));
        }
        let htype = match words.next() {
            Some(word) => try!(str::from_utf8(word)).to_owned(),
            None => {
                return Err(ParseError::MissingHashType);
            }
        };
        let hash = match words.next() {
            Some(word) => word.iter().cloned().map(|b| b - b'0').collect(),
            None => {
                return Err(ParseError::MissingHash);
            }
        };

        Ok((htype, hash))
    }
}


impl<'a> Iterator for WordIter<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.0.is_empty() {
                return None;
            }
            let pos = self.0.iter().position(|b| *b == b' ').unwrap_or(self.0.len());
            let (w, rest) = {
                let (w, rest) = self.0.split_at(pos);
                // skip all the spaces from rest
                let pos = rest.iter().position(|b| *b != b' ').unwrap_or(rest.len());
                (w, &rest[pos..])
            };
            self.0 = rest;
            if !w.is_empty() {
                return Some(w);
            }
        }
    }
}


fn unescape(mut buf: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(buf.len());
    buf = match (buf.first().cloned(), buf.last().cloned()) {
        // quoted
        (Some(b'"'), _) if buf.len() > 1 => &buf[1..buf.len() - 1],
        // unquoted or invalid single "
        (Some(_), _) => buf,
        // empty
        _ => {
            return result;
        }
    };

    // unescape
    let mut i = 0;
    let len = buf.len();
    while i < len {
        let b = buf[i];
        if b != b'\\' {
            result.push(b);
        } else {
            // expects a \xNN where NN is a number string representing the escaped char in hex
            // e.g. \x20 is the space ' '
            if buf.len() - i >= 4 && buf[i + 1] == b'x' {
                let num = ((buf[i + 2] - b'0') << 4) + buf[i + 3] - b'0';
                result.push(num);
                i += 3;
            }
        }
        i += 1;
    }

    result
}


#[cfg(test)]
mod test {
    use super::*;
    use std::fs::File;
    use std::io::BufReader;

    fn inc1_manifest() -> Result<Manifest, ParseError> {
        let file = File::open("tests/manifest/inc1.manifest").unwrap();
        let mut bfile = BufReader::new(file);
        Manifest::parse(&mut bfile)
    }


    #[test]
    fn parse_no_err_full() {
        let file = File::open("tests/manifest/full1.manifest").unwrap();
        let mut bfile = BufReader::new(file);
        Manifest::parse(&mut bfile).unwrap();
    }

    #[test]
    fn parse_no_err_inc() {
        inc1_manifest().unwrap();
    }

    #[test]
    fn first_volume_of_path() {
        let manifest = inc1_manifest().unwrap();
        assert_eq!(manifest.first_volume_of_path(b"home/michele/Immagini/Foto/albumfiles.txt")
                           .unwrap(),
                   28);
        assert_eq!(manifest.first_volume_of_path(b"home/michele/Documenti/Scuola/Open Class\
                                                 /Epfl/Principles of Reactive Programming/\
                                                 lectures/week7/lecture_slides_week7-1-annotated.pdf")
                           .unwrap(),
                   1);
        assert_eq!(manifest.first_volume_of_path(b"home/michele/Documenti/Scuola/Uni/\
                                                 Calcolo Numerico/octave docs/tutorial.pdf")
                           .unwrap(),
                   18);
    }
}
